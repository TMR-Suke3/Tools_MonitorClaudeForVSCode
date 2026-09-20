//! The live session registry — `~/.claude/sessions/<pid>.json`.
//!
//! One small JSON file per running Claude Code process, and the primary source of truth for
//! *which sessions exist right now* (`docs/observation-sources.md` §2.1). The core parses
//! one entry at a time, because that is how they occur on disk; reading the directory is the
//! outer layer's job.
//!
//! **A record on disk is not evidence that anything is running.** Measured: 3 live of 17
//! entries, and 14 of 84 lock files. Liveness is decided against the process table, which
//! reaches the core as data ([`LiveProcess`]) rather than as a call.

use serde::Deserialize;

use crate::record::Malformed;

/// One `sessions/<pid>.json`, reduced to the fields §2.1 names.
///
/// The credential-shaped fields are absent by construction: `messagingSocketPath` and
/// anything token-like is never named here, so it is never read (rule 2 of §1).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RegistryEntry {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// The workspace. The board's grouping key (FR-04).
    pub cwd: String,
    #[serde(rename = "startedAt", default)]
    pub started_at: Option<i64>,
    /// Process creation stamp, as Claude Code writes it: a Windows FILETIME in a string,
    /// counting 100-nanosecond intervals since 1601-01-01. Decoded by
    /// [`Self::proc_start_unix_seconds`]; kept verbatim so nothing is lost if the encoding
    /// turns out to vary.
    #[serde(rename = "procStart", default)]
    pub proc_start: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// `interactive` for a session a person is using.
    #[serde(default)]
    pub kind: Option<String>,
    /// `claude-vscode` for the sessions this board is about (FR-02).
    #[serde(default)]
    pub entrypoint: Option<String>,
    /// Fallback title when no `ai-title` has been written yet.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "nameSource", default)]
    pub name_source: Option<String>,
}

impl RegistryEntry {
    /// Whether this is a session the board is about: a VS Code session a person is using.
    ///
    /// **A strong hint, not proof.** `entrypoint` and `kind` are inherited from the
    /// environment rather than derived from how the process was started, so a `claude -p`
    /// run launched from inside a VS Code session's shell also reports `claude-vscode`
    /// (`docs/observation-sources.md` §2.1). An entry that does not state both is excluded:
    /// the filter's job is to keep non-editor sessions off the board, and silence is not a
    /// claim to be one.
    #[must_use]
    pub fn is_editor_session(&self) -> bool {
        self.entrypoint.as_deref() == Some("claude-vscode")
            && self.kind.as_deref() == Some("interactive")
    }

    /// The process creation stamp as **Unix seconds**, or `None` if it is not in the shape
    /// this product knows how to read.
    ///
    /// The registry writes a Windows FILETIME and the process table reports Unix time, so
    /// something has to convert; doing it here keeps the conversion in the crate that can be
    /// tested without a process table. Precision is deliberately dropped to whole seconds,
    /// because that is all the process table offers on the other side of the comparison.
    #[must_use]
    pub fn proc_start_unix_seconds(&self) -> Option<i64> {
        let raw = self.proc_start.as_deref()?;
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let filetime: i64 = raw.parse().ok()?;
        // 11_644_473_600 seconds separate 1601-01-01 from 1970-01-01.
        Some(filetime / 10_000_000 - 11_644_473_600)
    }

    /// Parses one registry file.
    ///
    /// # Errors
    ///
    /// Returns [`Malformed`] if the JSON is unreadable or is missing `pid`, `sessionId` or
    /// `cwd`. Callers ignore such an entry rather than reporting it as a failure (FR-40).
    pub fn parse(json: &str) -> Result<Self, Malformed> {
        serde_json::from_str(json).map_err(|e| Malformed {
            offset: 0,
            reason: e.to_string(),
        })
    }
}

/// A process the outer layer found in the process table.
///
/// The core never reads the process table itself; this is the observation, passed in
/// ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcess {
    pub pid: u32,
    /// When the process started, in **Unix seconds**, if the outer layer could read it.
    ///
    /// Unix seconds rather than the registry's own encoding because that is what a process
    /// table reports; converting on the registry side keeps the arithmetic where it can be
    /// tested from a recording.
    pub started_unix_seconds: Option<i64>,
}

impl LiveProcess {
    /// Whether this process is the one the entry describes.
    ///
    /// A PID match alone is not enough — PIDs are reused, and a stale registry file whose
    /// PID now belongs to an unrelated process would otherwise resurrect a dead session.
    /// The start times are therefore compared whenever both are known.
    ///
    /// **Within one second**, not exactly: the registry records 100-nanosecond FILETIME
    /// precision and a process table reports whole seconds, so the two encodings of the same
    /// instant can land either side of a boundary. One second is still a guard worth having
    /// — a PID reused inside the same second is not a case this product needs to survive.
    ///
    /// When one side cannot supply a start time the PID stands alone, which is weaker. The
    /// outer layer is expected to supply one, and that is where the guard actually lives.
    #[must_use]
    pub fn matches(&self, entry: &RegistryEntry) -> bool {
        if self.pid != entry.pid {
            return false;
        }
        match (self.started_unix_seconds, entry.proc_start_unix_seconds()) {
            (Some(a), Some(b)) => (a - b).abs() <= 1,
            _ => true,
        }
    }
}
