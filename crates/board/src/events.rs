//! Reading what the hook helper wrote.
//!
//! The helper (`crates/hook`) appends one line per event to a file in the product's own data
//! directory. This is the other end of that: the board reads the file, and the two agree on
//! four field names and nothing else.
//!
//! Two things this is for. The first is the proof at the end of setup — a successful write is
//! not success, and only an event that actually arrived says the feature works (FR-50). The
//! second is knowing when events *stop*: hooks that were installed and are no longer firing
//! must drop the board back to inference and say so, rather than leaving it presenting an
//! exactness it is no longer earning (FR-52).
//!
//! The file is shared by every Claude Code session on the machine, including terminal ones
//! the board does not display. That is by design — the helper cannot know which sessions the
//! board shows — and the board simply ignores the lines it has no session for.

use std::path::{Path, PathBuf};

use mcv_core::event::HookSignal;
use serde::Deserialize;

/// One line of the event log, with the same four fields the helper writes.
///
/// Deliberately an allow-list, exactly like the transcript parser: a future version of the
/// helper that wrote more would find nothing here willing to read it (NFR-07).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HookEvent {
    /// Milliseconds since the Unix epoch, stamped by the helper on arrival.
    pub at: i64,
    /// The event, as Claude Code names it.
    pub event: String,
    /// Which session it was about.
    pub session: String,
    /// Present for `Notification` only, and `permission_prompt` is the one that matters.
    #[serde(default)]
    pub kind: Option<String>,
}

/// The value of `kind` that means Claude is asking the user for something.
///
/// Two names for one thing, and it is worth being exact about which is which: Claude Code
/// calls the field `notification_type` in the payload it hands the helper, and the helper
/// writes it into the log as `kind` — see [`HookEvent`]. This constant is the *value*, which
/// is the same string on both sides.
///
/// The one value in the whole event log that can summon anybody, and the reason FR-49 was
/// amended to keep the field at all. Every other notification — most of them the
/// "this session has been idle a while" kind — is dropped: it is not a blocking UI, and
/// turning it amber would be inventing urgency (`session-state-model.md` §9).
pub const PERMISSION_PROMPT: &str = "permission_prompt";

impl HookEvent {
    /// This event in the state model's vocabulary, or `None` if the core has no use for it.
    ///
    /// **The translation lives here, on purpose.** These are Claude Code's names, and the
    /// core is not allowed to know them (ADR-0019): what crosses the boundary is a fact about
    /// the session, not a string this build happened to recognise.
    ///
    /// Honest about its evidence: only `Stop` has ever been observed as a real payload
    /// (`docs/observation-sources.md` §5, U-5 in the test plan). The other four names come
    /// from the documentation the setup was pinned against (FR-53), and an unrecognised one
    /// is dropped rather than guessed at — the helper writes what Claude Code hands it, and a
    /// name this build does not know is a name it must not act on.
    #[must_use]
    pub fn signal(&self) -> Option<HookSignal> {
        match self.event.as_str() {
            "SessionStart" => Some(HookSignal::Started),
            "UserPromptSubmit" => Some(HookSignal::PromptSubmitted),
            "Notification" => {
                (self.kind.as_deref() == Some(PERMISSION_PROMPT)).then_some(HookSignal::NeedsUser)
            }
            "Stop" => Some(HookSignal::TurnStopped),
            "SessionEnd" => Some(HookSignal::EndedCleanly),
            _ => None,
        }
    }
}

/// Where the helper writes, which must stay the same string in both crates.
///
/// The product's own data directory rather than Claude Code's: this file is ours, and
/// nothing that writes into `~/.claude/` should be written by a program the user did not ask
/// for.
pub const DATA_DIR: &str = "dev.tmr-suke3.mcv-board";

/// The event log's path, or `None` if the platform will not say where data goes.
#[must_use]
pub fn events_path() -> Option<PathBuf> {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(PathBuf::from(base).join(DATA_DIR).join("events.jsonl"))
}

/// Every event in the file that this build understands.
///
/// A line that will not parse is skipped rather than ending the read: the file is appended to
/// by several processes at once, so a torn last line is a normal thing to meet, not a fault
/// (FR-39, FR-40).
#[must_use]
pub fn read(path: &Path) -> Vec<HookEvent> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<HookEvent>(line).ok())
        .collect()
}

/// The newest stamp in the log, and the raw lines carrying it.
///
/// Both halves are needed to recognise history if the log is ever rewritten under the tail.
/// The stamp alone is not an identity — the helper stamps in whole milliseconds and several
/// processes append here — so the lines at that stamp are what a restart compares against
/// (`watch::live::HookLog`).
#[must_use]
pub fn newest_line_group(path: &Path) -> (i64, Vec<String>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (0, Vec::new());
    };
    let mut newest = 0;
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let Ok(event) = serde_json::from_str::<HookEvent>(line) else {
            continue;
        };
        if event.at > newest {
            newest = event.at;
            lines.clear();
        }
        if event.at == newest {
            lines.push(line.to_owned());
        }
    }
    (newest, lines)
}

/// When the most recent event arrived, of those written after `after`.
///
/// `after` is what makes this a proof rather than a coincidence: the setup asks whether an
/// event has arrived *since the entries were installed*, and a file left over from a previous
/// install would otherwise answer yes before anything had happened (FR-50).
#[must_use]
pub fn latest_since(path: &Path, after: i64) -> Option<i64> {
    read(path)
        .into_iter()
        .map(|event| event.at)
        .filter(|at| *at > after)
        .max()
}

/// Whether the board is currently earning the word "exact" (FR-52).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Hooks are installed and events are arriving.
    Exact,
    /// Hooks are installed, but nothing has arrived for a long time. The board says so and
    /// works from inference — an installed hook that is silent is indistinguishable from no
    /// hook at all, and pretending otherwise is the one failure this must not have.
    WentQuiet,
    /// No hooks. The ordinary case, and not a degraded one.
    Inferred,
}

/// How long silence is allowed to last before the board stops calling itself exact.
///
/// Generous on purpose. A session can sit untouched for an hour and nothing is wrong; what
/// this catches is the helper being deleted, moved, or blocked — where the silence never
/// ends. The cost of noticing late is small, and the cost of a false alarm is a board that
/// cries wolf about its own accuracy.
///
/// **The core's number, not a second copy of it.** The same silence that makes a session fall
/// back to inference is what makes the board say it has (FR-52), and the two must not be able
/// to disagree about when that happened.
pub const QUIET_AFTER_MILLIS: i64 = mcv_core::machine::HOOK_QUIET_MS;

impl Mode {
    /// The one word the board shows for this mode (FR-55).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::WentQuiet => "went-quiet",
            Self::Inferred => "inferred",
        }
    }
}

/// Works out the mode, given when the last event arrived.
#[must_use]
pub fn mode(installed: bool, last_event: Option<i64>, now: i64) -> Mode {
    if !installed {
        return Mode::Inferred;
    }
    match last_event {
        Some(at) if now.saturating_sub(at) <= QUIET_AFTER_MILLIS => Mode::Exact,
        // Installed but never heard from, or heard from long ago. Both are the same thing to
        // a user: the exact path is not working right now.
        _ => Mode::WentQuiet,
    }
}

/// Drops events older than `before`, so a log the board has been running beside all week does
/// not grow without limit (NFR-03).
///
/// Rewritten in one `write`, and only when something would actually go: this file is appended
/// to by live sessions, and rewriting it while one is mid-append loses that line. Losing a
/// line costs a single event, which the board falls back to inference for — but doing it on
/// every poll would make that constant.
///
/// # Errors
///
/// Any I/O failure. The caller ignores it: an event log that could not be trimmed is not a
/// reason to stop showing the board.
pub fn trim(path: &Path, before: i64) -> std::io::Result<usize> {
    let text = std::fs::read_to_string(path)?;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            // A line that will not parse has no age to judge, so it stays. Deleting what it
            // cannot read is the one thing this must never do.
            !serde_json::from_str::<HookEvent>(line).is_ok_and(|event| event.at < before)
        })
        .collect();

    let dropped = text.lines().count() - kept.len();
    if dropped == 0 {
        return Ok(0);
    }
    let mut out = kept.join("\n");
    out.push('\n');
    std::fs::write(path, out)?;
    Ok(dropped)
}
