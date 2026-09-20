//! The observation stream: everything the state machine is allowed to know.
//!
//! `docs/test-plan.md` §5.2 states the input precisely, and it is not transcript records
//! alone — it is *"the merge of transcript records, registry appearances and
//! disappearances, process-liveness changes, tool-process start/stop, subagent-file
//! activity, hook events and timer ticks"*, in the order an observer saw them.
//!
//! The vocabulary is not invented here. `tests/fixtures/prompt-observed.jsonl` is a recorded
//! merged stream — transcript records interleaved with process-table transitions under a
//! `{t, src, …}` envelope — and these types are shaped to hold it.
//!
//! **Everything arrives as data, the clock included**
//! ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
//! The machine never asks how long a call has been pending; it is told what time it is by
//! the events it is given, which is what makes a recording replayable.

use crate::record::Entry;
use crate::time::Timestamp;

/// One thing an observer saw, and when it saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// When the observation was made. For a transcript record this is the observer's clock,
    /// not the record's own `timestamp` — the two differ, which is the whole point of
    /// [ADR-0006](../../../docs/adr/0006-order-events-by-append-position.md).
    pub at: Timestamp,
    pub what: Event,
}

impl Observation {
    #[must_use]
    pub fn new(at: Timestamp, what: Event) -> Self {
        Self { at, what }
    }
}

/// What was observed.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A line was appended to the session's transcript.
    Record {
        /// Byte offset the line started at — the ordering key, never the timestamp.
        offset: u64,
        entry: Box<Entry>,
    },

    /// A process appeared for the pending tool call: the tool is genuinely running, so the
    /// call is work rather than a wait ([ADR-0008](../../../docs/adr/0008-detect-blocking-by-whether-the-tool-started.md)).
    ToolProcessStarted,
    /// That process ended. The result usually follows shortly after.
    ToolProcessExited,
    /// The probe looked and found **nothing running** for a pending call. Positive evidence
    /// that the tool has not started, and the reason a tool does not start is that something
    /// is blocking it.
    NoProcessForPendingCall,
    /// The probe could not answer — no permission, no platform support, a transient failure.
    ///
    /// **This is not evidence** (TC-15c). A platform that cannot say must resolve to the
    /// calmer status, never to a summons.
    ProbeUnavailable,

    /// A subagent's own `agent-*.jsonl` grew, so the parent's pending `Agent` call is a
    /// running subagent rather than a wait
    /// ([ADR-0007](../../../docs/adr/0007-detect-subagents-from-their-own-file.md)).
    SubagentFileGrew,

    /// The session's process is no longer running.
    ///
    /// The two fields are what separate a crash from a clean exit
    /// (`docs/session-state-model.md` §7.2), and both are `Option` because an observer that
    /// was not watching at the moment of death cannot know them.
    ProcessGone {
        /// Whether `sessions/<pid>.json` was **left behind**. A killed process leaves it; a
        /// clean exit removes it. `None` when the observer was not watching.
        ///
        /// A later disappearance proves nothing: any Claude Code process exiting cleanly
        /// sweeps *every* dead entry it finds, so entries vanish in bulk for unrelated
        /// reasons (measured: 17 collapsed to 3).
        entry_left_behind: Option<bool>,
        /// Whether the editor that hosted the session is still running. Used only when the
        /// registry evidence is missing.
        editor_alive: Option<bool>,
    },

    /// A line arrived that could not be read at all, or a transcript version this build does
    /// not understand (FR-39).
    ///
    /// Distinct from an unknown *record type*, which is skipped silently: this one means the
    /// observer has lost the thread and must say so. The session becomes `unknown` — live,
    /// but not understood — rather than disappearing or being guessed at (TC-13).
    Unreadable,

    /// A hook reported something about the session, authoritatively.
    ///
    /// The board translates Claude Code's event names into this vocabulary before the core
    /// sees them (`crates/board/src/events.rs`), for the same reason the clock arrives as a
    /// `Tick`: the core does not read files, and it does not know what Claude Code calls
    /// things either. What reaches it is a fact about the session.
    Hook(HookSignal),

    /// Time passed and nothing else happened.
    ///
    /// Timers are driven by these rather than by a clock the machine reads, so a recording
    /// replays identically however long the replay itself takes (NFR-11, TC-45a).
    Tick,
}

/// What a hook said, in the state model's terms rather than Claude Code's.
///
/// The right-hand column of `docs/session-state-model.md` §3, and nothing else: five events
/// are installed (`docs/hook-setup.md` §3.1) and each one answers exactly one question. A
/// notification that is not a permission prompt has no variant here — it never reaches the
/// core, because "Claude has been idle for a minute" is not a blocking UI and must not be
/// allowed to summon anybody (§9, *never invent urgency*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSignal {
    /// A session exists, exactly when it starts.
    ///
    /// Not activity: it says the session is there, not that it is doing something. What it
    /// does carry is the fact that hooks are alive for this session, which is what
    /// [`Event::Hook`] is worth on its own (FR-52).
    Started,
    /// A prompt was submitted: work has begun.
    PromptSubmitted,
    /// Claude is asking the user for permission or input, and cannot continue.
    ///
    /// **The reason the hook layer exists.** Inference reaches this from a pending call plus
    /// evidence that its tool never started (§4.1 rule 3); this is the session saying so.
    NeedsUser,
    /// The turn is over.
    TurnStopped,
    /// The session ended **cleanly**, which is the half §7.2 cannot read from a process
    /// disappearing. A session that ends this way is *absent*, never `terminated`.
    EndedCleanly,
}
