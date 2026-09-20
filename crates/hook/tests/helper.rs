//! What the hook helper writes, and — more importantly — what it refuses to (FR-48, NFR-07).
//!
//! These run the built binary rather than a function, because the two obligations that
//! matter are properties of the *process*: it must exit 0 whatever it is given, and the
//! conversation must never reach the event log.
//!
//! The second is worth stating exactly. The payload is read from standard input into a
//! `String`, so the conversation is in memory for the few milliseconds an invocation lasts.
//! What is checked here is that it is never deserialised into a field and never written —
//! which is what NFR-07 is a promise about, and what an external test can actually observe.
//!
//! The payloads are the shapes Claude Code's documentation gives, pinned on 2026-08-27 as
//! FR-53 requires. They are constructed rather than recorded: four of the five events have
//! never been observed on this machine (U-5 in `docs/test-plan.md` §6.2), which is exactly
//! why the setup flow refuses to install when a payload does not match what it expects.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// A scratch data directory, so nothing here goes near the real one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(case: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcv-hook-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        Self(dir)
    }

    /// The file the helper appends to.
    fn events(&self) -> PathBuf {
        self.0.join("dev.tmr-suke3.mcv-board").join("events.jsonl")
    }

    fn written(&self) -> String {
        std::fs::read_to_string(self.events()).unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn helper() -> PathBuf {
    // Cargo builds the binary for this crate before running its integration tests, and hands
    // the path over in this variable — extension and all.
    PathBuf::from(env!("CARGO_BIN_EXE_mcv-hook"))
}

/// Runs the helper with a payload on stdin, and returns its exit code.
fn run(scratch: &Scratch, args: &[&str], payload: &str) -> i32 {
    let mut child = Command::new(helper())
        .args(args)
        .env("APPDATA", &scratch.0)
        .env("XDG_DATA_HOME", &scratch.0)
        .env("HOME", &scratch.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the helper runs");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write the payload");
    child.wait().expect("the helper exits").code().unwrap_or(-1)
}

// ------------------------------------------------------------------ NFR-07

/// The conversation never reaches the file.
///
/// `UserPromptSubmit` carries `user_prompt` and `Stop` carries `last_assistant_message`.
/// Both are the thing this product exists not to store. Neither is named in the helper's
/// payload type, so serde steps over them and nothing downstream can write them.
#[test]
fn the_conversation_is_never_written() {
    let scratch = Scratch::new("content");

    let secret_prompt = "please refactor the authentication module and remove the old token";
    let secret_reply = "I have rewritten the token handling and deleted the legacy path";

    let code = run(
        &scratch,
        &["UserPromptSubmit"],
        &format!(
            r#"{{"session_id":"s1","prompt_id":"p1","transcript_path":"/x.jsonl","cwd":"/w",
                "permission_mode":"default","hook_event_name":"UserPromptSubmit",
                "user_prompt":"{secret_prompt}"}}"#
        ),
    );
    assert_eq!(code, 0);

    let code = run(
        &scratch,
        &["Stop"],
        &format!(
            r#"{{"session_id":"s1","prompt_id":"p1","transcript_path":"/x.jsonl","cwd":"/w",
                "permission_mode":"default","hook_event_name":"Stop",
                "last_assistant_message":"{secret_reply}"}}"#
        ),
    );
    assert_eq!(code, 0);

    let written = scratch.written();
    assert!(!written.is_empty(), "the events themselves are recorded");
    for secret in [secret_prompt, secret_reply] {
        assert!(
            !written.contains(secret),
            "the conversation reached the event log: {written}",
        );
    }
    // Nor anything else the payload happened to carry.
    for incidental in [
        "transcript_path",
        "/x.jsonl",
        "cwd",
        "permission_mode",
        "prompt_id",
    ] {
        assert!(
            !written.contains(incidental),
            "{incidental} was kept, and nothing needs it: {written}",
        );
    }
}

/// Four facts, and no more.
#[test]
fn a_line_holds_the_event_the_session_the_time_and_nothing_else() {
    let scratch = Scratch::new("shape");
    run(
        &scratch,
        &["Notification"],
        r#"{"session_id":"abc123","transcript_path":"/x.jsonl","cwd":"/w",
            "hook_event_name":"Notification","notification_type":"permission_prompt"}"#,
    );

    let written = scratch.written();
    let line: serde_json::Value = serde_json::from_str(written.trim()).expect("a JSON line");
    let mut keys: Vec<&str> = line
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    assert_eq!(keys, vec!["at", "event", "kind", "session"]);
    assert_eq!(line["event"], "Notification");
    assert_eq!(line["session"], "abc123");
    assert_eq!(
        line["kind"], "permission_prompt",
        "this is the moment the whole feature exists for: Claude asking the user something, \
         reported rather than inferred",
    );
    assert!(line["at"].as_i64().is_some_and(|at| at > 1_577_836_800_000));
}

// ------------------------------------------------------------------ FR-48

/// It exits 0 whatever it is handed. Every one of these is a real situation.
///
/// A hook that fails loudly interrupts the work it was installed to observe, and the first
/// thing the user will do about it is remove it.
#[test]
fn it_always_exits_zero() {
    let scratch = Scratch::new("exit");

    let cases: [(&str, &str); 6] = [
        ("nothing at all", ""),
        ("not json", "this is not json {{{"),
        ("json that is not an object", "[1, 2, 3]"),
        ("an object with nothing we need", r#"{"unrelated":true}"#),
        ("an event but no session", r#"{"hook_event_name":"Stop"}"#),
        (
            "an event this build has never heard of",
            r#"{"session_id":"s1","hook_event_name":"SomethingNew","brand_new_field":42}"#,
        ),
    ];

    for (what, payload) in cases {
        assert_eq!(run(&scratch, &["Stop"], payload), 0, "given {what}");
    }
}

/// A payload it cannot make sense of writes nothing rather than writing a guess (FR-53).
#[test]
fn an_unrecognisable_payload_writes_nothing() {
    let scratch = Scratch::new("silent");

    run(&scratch, &["Stop"], "not json at all");
    run(&scratch, &["Stop"], r#"{"unrelated":true}"#);
    run(&scratch, &["Stop"], r#"{"hook_event_name":"Stop"}"#);

    assert!(
        scratch.written().is_empty(),
        "a line was written for no session, which nothing on the board could use",
    );
}

/// The event may come from the command line, because that is where the installed entry puts
/// it: `"<install-dir>/mcv-hook.exe" <event> # monitor-claude-vscode v1` (hook-setup.md
/// §3.2). A payload that omits `hook_event_name` is therefore not an unreadable payload —
/// the invocation itself says which event fired.
#[test]
fn the_event_falls_back_to_the_command_line() {
    let scratch = Scratch::new("argv");
    run(&scratch, &["SessionEnd"], r#"{"session_id":"s1"}"#);

    let written = scratch.written();
    let line: serde_json::Value = serde_json::from_str(written.trim()).expect("a JSON line");
    assert_eq!(line["event"], "SessionEnd");
    assert_eq!(line["session"], "s1");
}

/// And the payload wins when the two disagree: it is the authority on what actually
/// happened, while the command line only says what the entry was installed for.
#[test]
fn the_payload_outranks_the_command_line() {
    let scratch = Scratch::new("disagree");
    run(
        &scratch,
        &["SessionStart"],
        r#"{"session_id":"s1","hook_event_name":"SessionEnd"}"#,
    );

    let written = scratch.written();
    let line: serde_json::Value = serde_json::from_str(written.trim()).expect("a JSON line");
    assert_eq!(line["event"], "SessionEnd");
}

/// A data directory that does not exist yet is created; one that cannot be is survived.
#[test]
fn a_missing_directory_is_made_and_an_impossible_one_is_survived() {
    let scratch = Scratch::new("dirs");
    assert!(!scratch.events().exists(), "nothing is there to begin with");

    run(
        &scratch,
        &["SessionStart"],
        r#"{"session_id":"s1","hook_event_name":"SessionStart"}"#,
    );
    assert!(
        scratch.events().exists(),
        "the helper made its own directory"
    );

    // A path that cannot be a directory, because a file is already sitting there.
    let blocked = std::env::temp_dir().join(format!("mcv-hook-blocked-{}", std::process::id()));
    std::fs::write(&blocked, b"not a directory").expect("write the blocker");
    let code = Command::new(helper())
        .args(["Stop"])
        .env("APPDATA", &blocked)
        .env("XDG_DATA_HOME", &blocked)
        .env("HOME", &blocked)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the helper runs")
        .code()
        .unwrap_or(-1);
    assert_eq!(
        code, 0,
        "an unwritable data directory is not the session's problem"
    );
    let _ = std::fs::remove_file(&blocked);
}

/// The event comes from the payload when it is there, and from the command line when it is
/// not — and the marker that follows it on the command line is ignored.
#[test]
fn the_marker_on_the_command_line_is_ignored() {
    let scratch = Scratch::new("marker");

    // Exactly what the settings entry runs, as the shell on Windows passes it: the `#` is
    // not a comment there, so the marker arrives as arguments.
    run(
        &scratch,
        &["Notification", "#", "monitor-claude-vscode", "v1"],
        r#"{"session_id":"s1","hook_event_name":"Notification","notification_type":"idle_prompt"}"#,
    );

    let written = scratch.written();
    assert!(
        !written.contains("monitor-claude-vscode"),
        "the marker is not data"
    );
    let line: serde_json::Value = serde_json::from_str(written.trim()).expect("a JSON line");
    assert_eq!(line["event"], "Notification");
}

/// Several events append rather than replace: the board reads this file forward.
#[test]
fn events_accumulate() {
    let scratch = Scratch::new("append");
    for event in ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"] {
        run(
            &scratch,
            &[event],
            &format!(r#"{{"session_id":"s1","hook_event_name":"{event}"}}"#),
        );
    }

    let written = scratch.written();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines.len(), 4, "one line per event, in order");
    assert!(lines[0].contains("SessionStart"));
    assert!(lines[3].contains("SessionEnd"));
}
