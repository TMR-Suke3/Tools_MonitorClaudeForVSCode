//! `mcv-hook` — the helper Claude Code runs when something happens in a session.
//!
//! It is deliberately the smallest program in this repository. Per invocation it reads one
//! JSON payload from standard input, keeps four facts from it, appends a line to a file, and
//! exits — and it **exits 0 whatever happens** (FR-48).
//!
//! That last rule is not politeness. This runs inside the user's session, on every prompt
//! and every reply. A hook that fails loudly is a hook that interrupts the work it was
//! installed to observe, and the first thing the user will do about it is remove it. So a
//! missing directory, an unreadable payload, a full disk and a board that is not running all
//! end the same way: silently, successfully, having done nothing.
//!
//! # What it keeps, and what it refuses to look at
//!
//! Two of the five payloads carry the conversation itself — `UserPromptSubmit` delivers
//! `user_prompt` and `Stop` delivers `last_assistant_message`. **Neither is named below**, so
//! serde steps over them: they are never deserialised, never kept, and never written
//! (NFR-07). This is the same allow-list discipline the transcript parser uses, and it
//! matters more here: a transcript is a file this product chose to open, while a hook
//! payload is handed to it.
//!
//! Be precise about the strength of that. The payload arrives on standard input and is read
//! into a `String`, so the conversation does pass through this process's memory for as long
//! as one invocation lasts — a few milliseconds, and then the process exits. What the
//! allow-list guarantees is that nothing here can *act* on it: it is not parsed into a
//! field, not held, and not written anywhere.
//!
//! The event names and payload shapes were pinned against Claude Code's documentation on
//! 2026-08-27, as FR-53 requires. If a payload does not match, the line is not written.

use std::io::{Read, Write};

use serde::Deserialize;

/// The fields this product is allowed to see. Anything else in the payload is stepped over.
#[derive(Deserialize)]
struct Payload {
    /// Which session this is about. The only identifier kept.
    #[serde(default)]
    session_id: Option<String>,
    /// The event, as Claude Code names it. Taken from the payload rather than the command
    /// line where it is present, because the payload is the authority on what happened.
    #[serde(default)]
    hook_event_name: Option<String>,
    /// Why a `Notification` fired. `permission_prompt` is the one this whole feature exists
    /// for — the moment Claude asks the user for something, reported rather than inferred.
    #[serde(default)]
    notification_type: Option<String>,
}

/// One line of the event log: four facts, and nothing that could be read back as content.
#[derive(serde::Serialize)]
struct Line<'a> {
    /// Milliseconds since the Unix epoch, from this machine's clock.
    at: i64,
    event: &'a str,
    session: &'a str,
    /// Present only for `Notification`.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'a str>,
}

fn main() {
    // Every path below ends here, including the ones that did nothing at all.
    record();
    std::process::exit(0);
}

fn record() {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return;
    }
    let Ok(payload) = serde_json::from_str::<Payload>(&raw) else {
        // A payload this build does not understand is not written as a guess (FR-53).
        return;
    };

    // The command line carries the event as a fallback, since the settings entry names it:
    //   "<install-dir>/mcv-hook.exe" Notification # monitor-claude-vscode v1
    // The marker after it is what makes the entry ours, and arrives here as extra arguments
    // that nothing reads. On Windows the shell does not treat `#` as a comment, which is why
    // they arrive at all rather than being stripped.
    let from_argv = std::env::args().nth(1);
    let event = payload
        .hook_event_name
        .as_deref()
        .or(from_argv.as_deref())
        .unwrap_or("");
    let session = payload.session_id.as_deref().unwrap_or("");
    if event.is_empty() || session.is_empty() {
        return;
    }

    let line = Line {
        at: now_millis(),
        event,
        session,
        kind: payload.notification_type.as_deref(),
    };
    let Ok(mut text) = serde_json::to_string(&line) else {
        return;
    };
    text.push('\n');

    let Some(path) = events_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Append, and ignore the outcome. Several sessions write here at once; a line lost to a
    // collision costs one event, and the board falls back to inference for it (FR-52).
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Where the board looks for events.
///
/// The product's own data directory, not Claude Code's: this file is ours, and nothing that
/// writes into `~/.claude/` should be written by a program the user did not ask for.
fn events_path() -> Option<std::path::PathBuf> {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(
        std::path::PathBuf::from(base)
            .join("dev.tmr-suke3.mcv-board")
            .join("events.jsonl"),
    )
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
