//! Reading the helper's event log — the other half of `crates/hook`.
//!
//! The two crates agree on four field names and a path, and nothing else. These tests write
//! the lines a helper would write and check that the board reads them back the same way —
//! including the lines it should not read at all.

use std::path::PathBuf;

use mcv_board::events::{self, HookEvent, Mode, PERMISSION_PROMPT, QUIET_AFTER_MILLIS};
use mcv_core::event::HookSignal;

struct Scratch(PathBuf);

impl Scratch {
    fn new(case: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcv-events-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        Self(dir)
    }

    fn with(&self, text: &str) -> PathBuf {
        let path = self.0.join("events.jsonl");
        std::fs::write(&path, text).expect("write the log");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The four fields the helper writes, and what the board makes of them.
#[test]
fn it_reads_what_the_helper_writes() {
    let scratch = Scratch::new("read");
    let path = scratch.with(concat!(
        r#"{"at":1000,"event":"SessionStart","session":"s1"}"#,
        "\n",
        r#"{"at":2000,"event":"Notification","session":"s1","kind":"permission_prompt"}"#,
        "\n",
    ));

    let events = events::read(&path);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event, "SessionStart");
    assert_eq!(events[0].kind, None, "only a Notification carries a kind");
    assert_eq!(
        events[1].kind.as_deref(),
        Some("permission_prompt"),
        "the moment the whole feature exists for",
    );
}

/// A torn last line is what a file several processes append to looks like mid-write. It is
/// skipped, and the lines around it are still read (FR-39, FR-40).
#[test]
fn a_broken_line_does_not_end_the_read() {
    let scratch = Scratch::new("torn");
    let path = scratch.with(concat!(
        r#"{"at":1000,"event":"Stop","session":"s1"}"#,
        "\n",
        r#"{"at":2000,"event":"Sto"#,
        "\n",
        r#"{"at":3000,"event":"SessionEnd","session":"s1"}"#,
        "\n",
    ));

    let events = events::read(&path);
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].at, 3000);
}

/// A file that is not there is not an error: hooks are optional, and their absence is the
/// ordinary case rather than a degraded one.
#[test]
fn a_missing_log_reads_as_no_events() {
    let scratch = Scratch::new("missing");
    assert!(events::read(&scratch.0.join("nothing.jsonl")).is_empty());
}

/// The proof at the end of setup asks for an event *since the write* (FR-50).
///
/// Without the cutoff, a log left behind by a previous install answers "yes" the moment the
/// window opens — which is the exact failure FR-50 exists to prevent: a setup that reports
/// success without having verified anything.
#[test]
fn an_old_log_is_not_proof_that_a_new_install_works() {
    let scratch = Scratch::new("since");
    let path = scratch.with(concat!(
        r#"{"at":1000,"event":"Stop","session":"s1"}"#,
        "\n",
        r#"{"at":2000,"event":"Stop","session":"s1"}"#,
        "\n",
    ));

    assert_eq!(events::latest_since(&path, 0), Some(2000));
    assert_eq!(events::latest_since(&path, 1500), Some(2000));
    assert_eq!(
        events::latest_since(&path, 2000),
        None,
        "an event at the moment of the write is not evidence about the write",
    );
    assert_eq!(events::latest_since(&path, 9999), None);
}

/// Installed and silent is not exact (FR-52).
#[test]
fn silence_drops_the_board_back_to_inference() {
    let now = 10 * QUIET_AFTER_MILLIS;

    assert_eq!(
        events::mode(false, None, now),
        Mode::Inferred,
        "no hooks, and that is fine"
    );
    assert_eq!(
        events::mode(false, Some(now), now),
        Mode::Inferred,
        "an event log left over from an install that has been removed says nothing",
    );

    assert_eq!(events::mode(true, Some(now - 1000), now), Mode::Exact);
    assert_eq!(
        events::mode(true, Some(now - QUIET_AFTER_MILLIS), now),
        Mode::Exact,
        "exactly at the threshold is still within it",
    );
    assert_eq!(
        events::mode(true, Some(now - QUIET_AFTER_MILLIS - 1), now),
        Mode::WentQuiet,
    );
    assert_eq!(
        events::mode(true, None, now),
        Mode::WentQuiet,
        "installed and never heard from is the same to a user as installed and stopped",
    );
}

/// Trimming drops what is old and keeps what it cannot read.
#[test]
fn trimming_never_deletes_a_line_it_could_not_parse() {
    let scratch = Scratch::new("trim");
    let path = scratch.with(concat!(
        r#"{"at":1000,"event":"Stop","session":"s1"}"#,
        "\n",
        "this line is not JSON\n",
        r#"{"at":5000,"event":"Stop","session":"s1"}"#,
        "\n",
    ));

    let dropped = events::trim(&path, 4000).expect("trims");
    assert_eq!(dropped, 1);

    let text = std::fs::read_to_string(&path).expect("read back");
    assert!(!text.contains("\"at\":1000"), "the old event is gone");
    assert!(text.contains("\"at\":5000"), "the recent one stays");
    assert!(
        text.contains("this line is not JSON"),
        "a line with no age to judge is kept: deleting what it cannot read is the one thing \
         this must never do",
    );
}

/// Nothing old means nothing written, so a log that is appended to constantly is not
/// rewritten under a live session on every poll.
#[test]
fn trimming_nothing_writes_nothing() {
    let scratch = Scratch::new("untrimmed");
    let path = scratch.with(concat!(
        r#"{"at":5000,"event":"Stop","session":"s1"}"#,
        "\n",
    ));
    let before = std::fs::metadata(&path).expect("metadata").modified().ok();

    assert_eq!(events::trim(&path, 1000).expect("trims"), 0);
    assert_eq!(
        std::fs::metadata(&path).expect("metadata").modified().ok(),
        before,
        "the file was not touched",
    );
}

// ------------------------------------------------ TC-93: Claude Code's names, translated

/// One line of the log, as the helper writes it.
fn line(event: &str, kind: Option<&str>) -> HookEvent {
    let kind = kind.map_or_else(String::new, |k| format!(r#","kind":"{k}""#));
    let text = format!(r#"{{"at":1,"event":"{event}","session":"s1"{kind}}}"#);
    serde_json::from_str(&text).expect("the helper's own shape")
}

/// TC-93 (FR-36) — the five installed events, in the state model's terms.
///
/// This is the whole of the board's half of hook mode: Claude Code's vocabulary stops here,
/// and what crosses into the core is a fact about the session (ADR-0019).
///
/// **Unverified against real payloads for four of the five.** Only `Stop` has ever been
/// observed (U-5, `docs/observation-sources.md` §5); the rest come from the documentation the
/// setup was pinned against (FR-53). What this test pins is the mapping, not the names.
#[test]
fn tc_93_the_five_events_become_five_signals() {
    assert_eq!(
        line("SessionStart", None).signal(),
        Some(HookSignal::Started)
    );
    assert_eq!(
        line("UserPromptSubmit", None).signal(),
        Some(HookSignal::PromptSubmitted),
    );
    assert_eq!(line("Stop", None).signal(), Some(HookSignal::TurnStopped));
    assert_eq!(
        line("SessionEnd", None).signal(),
        Some(HookSignal::EndedCleanly)
    );
    assert_eq!(
        line("Notification", Some(PERMISSION_PROMPT)).signal(),
        Some(HookSignal::NeedsUser),
        "the moment the whole feature exists for",
    );
}

/// TC-94 (FR-13) — only a permission prompt summons anybody.
///
/// Claude Code notifies for other reasons, the commonest being a session that has been
/// sitting idle. That is not a blocking UI: turning it amber would be inventing urgency
/// (`session-state-model.md` §9), and the amended FR-49 keeps `notification_type` precisely
/// so the two can be told apart.
#[test]
fn tc_94_a_notification_that_is_not_a_prompt_is_dropped() {
    for kind in [None, Some("idle"), Some("elicitation"), Some("")] {
        assert_eq!(
            line("Notification", kind).signal(),
            None,
            "a Notification of kind {kind:?} must not reach the core",
        );
    }
}

/// An event name this build does not know is dropped, not guessed at.
///
/// The helper writes whatever Claude Code hands it, and Claude Code's vocabulary is a moving
/// target — parse defensively, and never act on a name whose meaning is not pinned.
#[test]
fn an_unknown_event_name_means_nothing() {
    assert_eq!(line("PreToolUse", None).signal(), None);
    assert_eq!(line("", None).signal(), None);
    assert_eq!(line("stop", None).signal(), None, "the names are exact");
}

/// The word the board shows for each mode (FR-55).
#[test]
fn every_mode_has_a_word() {
    assert_eq!(Mode::Exact.label(), "exact");
    assert_eq!(Mode::WentQuiet.label(), "went-quiet");
    assert_eq!(Mode::Inferred.label(), "inferred");
}

/// The board and the sessions fall back on the same silence.
///
/// A board still saying "exact" while its sessions had gone back to inference would be the
/// exact failure FR-52 names, so the number has one home and both sides read it.
#[test]
fn the_board_and_the_core_agree_on_when_silence_is_too_long() {
    assert_eq!(QUIET_AFTER_MILLIS, mcv_core::machine::HOOK_QUIET_MS);
}

/// The newest stamp, and the lines that carry it.
///
/// Both halves matter: a board that has just started knows how far the log had got, and if
/// the log is rewritten before anything new is appended, the lines are the only way to tell
/// the history it deliberately skipped from an event written since.
#[test]
fn the_newest_stamp_comes_with_the_lines_that_carry_it() {
    let scratch = Scratch::new("newest");
    let a = r#"{"at":700,"event":"Stop","session":"s1"}"#;
    let b = r#"{"at":700,"event":"SessionEnd","session":"s2"}"#;
    let path = scratch.with(&format!(
        "{}\n{}\n{}\n{}\n",
        r#"{"at":100,"event":"SessionStart","session":"s1"}"#, a, "not a line anything can read", b,
    ));

    let (newest, lines) = events::newest_line_group(&path);
    assert_eq!(newest, 700);
    assert_eq!(
        lines,
        vec![a.to_owned(), b.to_owned()],
        "two events can share a millisecond, and both are the boundary",
    );

    let (empty_at, empty_lines) = events::newest_line_group(&scratch.0.join("nothing.jsonl"));
    assert_eq!(
        empty_at, 0,
        "a log that is not there has no newest anything"
    );
    assert!(empty_lines.is_empty());
}
