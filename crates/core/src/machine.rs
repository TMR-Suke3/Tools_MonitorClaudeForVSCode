//! The status state machine (`docs/session-state-model.md`).
//!
//! One [`Machine`] per session. It is a fold over the observation stream: every input is
//! data, including the passage of time, so replaying a recording twice gives the same answer
//! and replaying it record-by-record gives the same answer as replaying it in one go
//! (NFR-11, TC-45).
//!
//! **Exactly one status at every point** (FR-08). The machine holds a status, not a set of
//! flags that a renderer later has to reconcile.

use crate::event::{Event, HookSignal, Observation};
use crate::palette::Status;
use crate::record::{Entry, Record, StopReason, SystemNotice};
use crate::time::Timestamp;

// ---------------------------------------------------------------- thresholds

/// The timers of `docs/session-state-model.md` §3.
///
/// "All are configurable; the defaults are the specification." They are a struct rather
/// than constants for that reason — and because `T_pending_probe` is explicitly the
/// least-evidenced number in that document, so it must be changeable without touching the
/// rules that read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// Only used when a turn stops without a readable stop reason. The normal path is
    /// structural and does not wait.
    pub idle_fallback_ms: i64,
    /// Debounce before calling an always-interactive tool call a wait.
    pub pending_interactive_ms: i64,
    /// How long a pending call is given to show a running process before its absence counts
    /// as evidence.
    ///
    /// **Unverified.** `session-state-model.md` §3 says so itself: it covers a measured
    /// 2.3 s startup lag on a single `Bash` call, with a deliberately generous margin, and
    /// phase 4 is supposed to measure the distribution properly before this number is
    /// trusted. That measurement has not happened. Nothing else in the rules depends on its
    /// value, only on there being one.
    pub pending_probe_ms: i64,
    /// How long a *vanished* session stays on the board before it clears itself (FR-59).
    pub terminated_visible_ms: i64,
    /// How long an in-process tool may be pending before it counts as a gross outlier for
    /// its own tool (§4.1 rule 3, second bullet).
    ///
    /// **Also unverified as a distribution.** The measurement behind it is that `Edit` calls
    /// exceeding 45 s ran 48× to 3,239× that tool's own median, while `Bash` and
    /// `PowerShell` are legitimately slow at the same durations — which is why this applies
    /// only to tools that never spawn a process.
    pub inprocess_outlier_ms: i64,
    /// How long the hook layer may stay silent before the session stops believing it and
    /// goes back to inference (FR-52, §3's `T_hook_quiet`).
    ///
    /// Generous on purpose, and the reason is that silence is ambiguous: a session can sit
    /// untouched for an hour with nothing wrong. What this catches is the helper being
    /// deleted, moved or blocked — a silence that never ends. Believing a dead hook layer is
    /// the one failure this must not have; noticing it late costs nothing, because inference
    /// is what would have run anyway.
    pub hook_quiet_ms: i64,
}

/// `T_hook_quiet`'s default, and the **one** place the number is written.
///
/// The board's own mode indicator reads it from here rather than keeping a second copy: a
/// session that has fallen back to inference and a board that says it has must not be able
/// to disagree.
pub const HOOK_QUIET_MS: i64 = 30 * 60 * 1_000;

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            idle_fallback_ms: 3_000,
            pending_interactive_ms: 1_500,
            pending_probe_ms: 10_000,
            terminated_visible_ms: 600_000,
            inprocess_outlier_ms: 45_000,
            hook_quiet_ms: HOOK_QUIET_MS,
        }
    }
}

/// Tools whose entire purpose is to ask the user something. A pending call to one of these
/// is a wait after a short debounce, with no process evidence needed (§4.1 rule 2).
pub const ALWAYS_INTERACTIVE: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

/// Tools that run inside the Claude Code process and therefore never show up as a process.
///
/// For these, "pending far longer than this tool has ever taken" is meaningful where a fixed
/// threshold is not — and the process probe can say nothing at all, because there was never
/// going to be a process to find.
pub const IN_PROCESS_TOOLS: [&str; 8] = [
    "Edit",
    "Write",
    "Read",
    "NotebookEdit",
    "TodoWrite",
    "Glob",
    "Grep",
    "WebFetch",
];

/// Permission modes that never prompt, so a pending call under them is always work
/// (§4.1 rule 1).
///
/// **Unverified against a recording.** All 78 transcripts in the corpus say `normal`
/// (U-4 in `docs/test-plan.md` §6.2), so this list is taken from Claude Code's own
/// vocabulary rather than from data. Getting it wrong is safe in one direction only: a mode
/// wrongly treated as prompting can still produce a wait, while a mode wrongly treated as
/// silent could never do so.
pub const NON_PROMPTING_MODES: [&str; 1] = ["bypassPermissions"];

// ---------------------------------------------------------------- what comes out

/// How sure the machine is. Only two levels, because only two are earned: a signal either
/// rests on positive evidence or it does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Rests on positive evidence — a stop reason, a quota rejection, an absent process.
    High,
    /// Structural but weaker: a fallback, or a status reached by the absence of anything
    /// better.
    Inferred,
}

/// Extra facts a status carries that the board cannot derive for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Detail {
    /// Both ends of a usage-limit wait (TC-25e): when the rejection that named it happened,
    /// and when the session may continue.
    ///
    /// Both are needed. A display showing how much of the wait is left cannot derive the
    /// length from the reset alone, and only the core sees the transcript the rejection
    /// arrived in. The *remaining* time needs the current clock and is therefore not the
    /// core's to compute (§6).
    pub limit: Option<Limit>,
}

/// A usage-limit window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    /// When the first rejection that named this window arrived.
    pub rejected_at: Timestamp,
    /// When the session may continue, exactly as the record stated it. Read, never computed:
    /// measured gaps ran from 5.4 to 215.3 minutes, so any fixed offset is wrong by hours.
    pub resets_at: Option<i64>,
}

/// A transition. Emitted only when the status actually changes — a status that stays put
/// produces nothing, which is what keeps a sub-second tool call from flickering (TC-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusChange {
    pub at: Timestamp,
    pub from: Status,
    pub to: Status,
    pub confidence: Confidence,
    pub detail: Detail,
}

// ---------------------------------------------------------------- the machine

/// A tool call that has been announced and not yet answered.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    id: String,
    name: String,
    /// When the observer saw the call announced — not the record's own timestamp, because
    /// the thresholds are about how long *the observer* has been waiting.
    since: Timestamp,
}

/// The status of one session, folded from its observation stream.
#[derive(Debug, Clone)]
pub struct Machine {
    thresholds: Thresholds,
    status: Status,
    confidence: Confidence,
    detail: Detail,
    /// Outstanding tool calls, in the order they were announced.
    pending: Vec<Pending>,
    /// The newest **timestamped** record seen, which is what time-in-status is measured
    /// from (TC-18) and what makes a late append recognisable (TC-21).
    newest_record_at: Option<Timestamp>,
    /// Set when the probe reported nothing running for a pending call.
    ///
    /// Cleared when a process appears, and **whenever any outstanding call is answered** —
    /// not only when the last one is. That is deliberate, and it is the conservative
    /// direction:
    ///
    /// The recorded process events carry no tool id (see `tests/fixtures/prompt-observed.jsonl`),
    /// so this flag is one fact about the whole pending set rather than a fact about a
    /// particular call. Once the set changes, the observation was taken about a different
    /// question. Carrying it forward would let evidence gathered while an *in-process* tool
    /// was outstanding — for which "nothing is running" means nothing at all, since one never
    /// spawns a process — summon the user about a subprocess tool that is merely slow.
    ///
    /// The cost is a delay, not a missed summons: the observer keeps probing, so a call that
    /// really is blocked is reported again on the next cycle. §4.1's bias is exactly this —
    /// absent or stale evidence resolves to `working`, never to a summons.
    ///
    /// Two calls outstanding at once is not hypothetical: it occurs in the corpus.
    no_process_since: Option<Timestamp>,
    /// A subagent file is growing, so a pending call is a running subagent (ADR-0007).
    subagent_growing: bool,
    /// Whether the session's permission mode can prompt at all (§4.1 rule 1).
    mode_can_prompt: bool,
    /// When the session's process was seen to be **gone**, so its slot can clear itself
    /// after `T_terminated_visible` (FR-59).
    ///
    /// Named for the vanishing rather than for the status, and set in exactly one place, so
    /// that the other kind of abnormal stop cannot acquire a timer by accident. §7.1's case —
    /// the turn died but the session is still there, and the user can type again — has no
    /// timer at all: new activity is what clears it. Evicting a live session from the board
    /// after ten minutes would hide a session that still exists, which is the opposite of
    /// what this product is for.
    vanished_since: Option<Timestamp>,
    /// When a hook last reported anything about this session.
    ///
    /// Two jobs, and they are the same fact seen from either end. While it is recent, the
    /// hook layer is alive and §4.1 rule 4 applies — the hook decides and rules 2–3 are
    /// skipped. Once it is older than `T_hook_quiet`, the hooks have gone silent and the
    /// session falls back to inference on its own (FR-52), without anything having to tell
    /// it to.
    hook_last: Option<Timestamp>,
    /// A hook said Claude is asking the user for something, and nothing has said otherwise.
    ///
    /// Held as a **fact about the session** rather than written straight to the status, and
    /// that is the whole design ([ADR-0020](../../../docs/adr/0020-hook-signals-are-facts-not-status-writes.md)):
    /// a hook line and a transcript line read in the same poll have no order that the
    /// observer can defend, so the answer must not depend on which is folded in first.
    hook_wait: bool,
    /// Set once the session should leave the board.
    ended: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    /// A session that has just appeared. It starts `working`: something opened it.
    #[must_use]
    pub fn new() -> Self {
        Self::with_thresholds(Thresholds::default())
    }

    #[must_use]
    pub fn with_thresholds(thresholds: Thresholds) -> Self {
        Self {
            thresholds,
            status: Status::Working,
            confidence: Confidence::Inferred,
            detail: Detail::default(),
            pending: Vec::new(),
            newest_record_at: None,
            no_process_since: None,
            subagent_growing: false,
            mode_can_prompt: true,
            vanished_since: None,
            hook_last: None,
            hook_wait: false,
            ended: false,
        }
    }

    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    #[must_use]
    pub const fn detail(&self) -> Detail {
        self.detail
    }

    /// Whether the session has ended and should leave the board — either cleanly, or after
    /// `T_terminated_visible` has elapsed on an abnormal stop (FR-59).
    #[must_use]
    pub const fn has_ended(&self) -> bool {
        self.ended
    }

    /// Folds one observation in, returning the transition if the status changed.
    pub fn observe(&mut self, observation: &Observation) -> Option<StatusChange> {
        let before = (self.status, self.confidence, self.detail);

        match &observation.what {
            Event::Record { entry, .. } => self.on_record(observation.at, entry),
            Event::ToolProcessStarted => {
                self.no_process_since = None;
                // A tool that is running is a prompt that was answered: §4.1 names this as
                // one of the two ways a wait ends. It ends a *reported* wait for the same
                // reason it ends an inferred one — the thing that was blocked is no longer
                // blocked — and waiting for the `tool_result` instead would leave the board
                // amber for as long as the tool takes to run.
                self.wait_ended();
            }
            Event::ToolProcessExited => {}
            Event::NoProcessForPendingCall => {
                if self.no_process_since.is_none() {
                    self.no_process_since = Some(observation.at);
                }
            }
            // Deliberately nothing: the platform failing to answer is not evidence (TC-15c).
            Event::ProbeUnavailable => {}
            Event::SubagentFileGrew => {
                self.subagent_growing = true;
                self.enter(Status::Working, Confidence::High);
            }
            Event::ProcessGone {
                entry_left_behind,
                editor_alive,
            } => self.on_process_gone(observation.at, *entry_left_behind, *editor_alive),
            Event::Unreadable => self.enter(Status::Unknown, Confidence::Inferred),
            Event::Hook(signal) => self.on_hook(observation.at, *signal),
            Event::Tick => {}
        }

        self.reconsider_pending(observation.at);
        self.expire_abnormal_stop(observation.at);

        let after = (self.status, self.confidence, self.detail);
        (before.0 != after.0).then_some(StatusChange {
            at: observation.at,
            from: before.0,
            to: after.0,
            confidence: after.1,
            detail: after.2,
        })
    }

    // ------------------------------------------------------------ transcript

    fn on_record(&mut self, at: Timestamp, entry: &Entry) {
        // A record appended after the turn it belongs to, carrying an older stamp, is not
        // new evidence — it is history arriving late (ADR-0006, TC-21).
        let is_late_append = match (entry.meta.at, self.newest_record_at) {
            (Some(record_at), Some(newest)) => record_at < newest,
            _ => false,
        };
        if let Some(record_at) = entry.meta.at {
            if self
                .newest_record_at
                .is_none_or(|newest| record_at > newest)
            {
                self.newest_record_at = Some(record_at);
            }
        }

        // The usage limit rides on an ordinary record and outranks the rest of it (§6).
        if let Some(quota) = &entry.quota {
            if quota.is_rejected() {
                let limit = Limit {
                    rejected_at: entry.meta.at.unwrap_or(at),
                    resets_at: quota.resets_at,
                };
                // A repeated rejection while still limited keeps the original start, so the
                // pair stays stable for the whole wait.
                if self.status != Status::Limited {
                    self.detail.limit = Some(limit);
                }
                self.enter(Status::Limited, Confidence::High);
                return;
            }
        }

        match &entry.record {
            Record::Assistant(message) => {
                for (id, name) in message.tool_uses() {
                    self.pending.push(Pending {
                        id: id.to_owned(),
                        name: name.to_owned(),
                        since: at,
                    });
                }
                self.activity();
                // A turn ends structurally, on a stop reason — never on a timeout. A null
                // reason is a mid-stream partial record and ends nothing (TC-10).
                let ends_turn = matches!(
                    message.stop_reason,
                    StopReason::EndTurn | StopReason::StopSequence
                );
                if ends_turn && self.pending.is_empty() {
                    self.enter(Status::Idle, Confidence::High);
                }
            }
            Record::User(message) => {
                let mut answered_any = false;
                for (tool_use_id, _is_error) in message.tool_results() {
                    // A refusal answers the call exactly as an approval does: both end the
                    // wait, which is why `awaiting_user` needs no timeout to escape (TC-15e).
                    let before = self.pending.len();
                    self.pending.retain(|p| p.id != tool_use_id);
                    answered_any |= self.pending.len() != before;
                }
                // Any answer changes the pending set, and the probe's answer was about the
                // set as it was. Stale evidence is dropped rather than reused — see the note
                // on `no_process_since`.
                if answered_any || self.pending.is_empty() {
                    self.no_process_since = None;
                    self.subagent_growing = false;
                }
                // The other way a wait ends: a refusal writes a `tool_result` carrying an
                // error, exactly as an approval eventually writes an ordinary one. Only when
                // **nothing** is left outstanding, though — a `tool_result` answering one of
                // two calls says nothing about the other, and the other is the one a hook
                // reported a prompt for.
                if answered_any && self.pending.is_empty() {
                    self.wait_ended();
                }
                self.activity();
            }
            Record::System(notice) => self.on_notice(notice, is_late_append),
            Record::QueueOperation(_) => self.activity(),
            Record::Mode(mode) => {
                self.mode_can_prompt = !NON_PROMPTING_MODES.contains(&mode.as_str());
            }
            // Not in §3's list of activity, and deliberately so: a session being retitled is
            // not a session doing something, and must not resurrect an idle one.
            Record::AiTitle(_)
            | Record::LastPrompt
            | Record::Attachment
            | Record::Ignored { .. } => {}
        }
    }

    fn on_notice(&mut self, notice: &SystemNotice, is_late_append: bool) {
        match notice {
            SystemNotice::ApiError {
                retry_attempt,
                max_retries,
                ..
            } => {
                if is_late_append {
                    // The turn it belonged to is already over. Nothing to resurrect.
                    return;
                }
                let exhausted = match (retry_attempt, max_retries) {
                    (Some(attempt), Some(max)) => attempt >= max,
                    _ => false,
                };
                if exhausted {
                    self.enter(Status::Terminated, Confidence::High);
                } else {
                    // Retrying *is* activity. The board must not cry wolf on a hiccup.
                    self.activity();
                }
            }
            // Compaction is work, not a pause (TC-26).
            SystemNotice::CompactBoundary { .. } => self.activity(),
            SystemNotice::StopHookSummary { .. } | SystemNotice::Other(_) => {}
        }
    }

    /// Any recognised activity: it clears a limit, clears an abnormal stop, and wakes an
    /// idle session (§3).
    fn activity(&mut self) {
        if self.status != Status::Working {
            self.detail.limit = None;
            self.enter(Status::Working, Confidence::High);
        }
    }

    // ------------------------------------------------------------ hook mode (§3)

    /// Folds in what a hook said.
    ///
    /// The right-hand column of `docs/session-state-model.md` §3. Hook mode "changes *how
    /// fast and how certainly* a transition is known, never *what the statuses mean*", so
    /// every arm below lands on a status the inference path can also reach — it just gets
    /// there from the session's own report instead of from two indirect signals.
    fn on_hook(&mut self, at: Timestamp, signal: HookSignal) {
        // Every signal, including the ones that change nothing, is evidence that the hook
        // layer is alive. That is what FR-52 measures.
        self.hook_last = Some(at);

        match signal {
            // A session existing is not a session doing something. It arrives at the moment
            // the session starts, when the board already shows it as `working`, and the only
            // thing it adds is the stamp above.
            // A session that is starting has nothing outstanding, so anything left over from
            // before is stale by definition. This matters most where it is least obvious: if
            // the hook layer goes silent mid-prompt and comes back days later, this is the
            // signal that clears the wait it left behind.
            HookSignal::Started => self.forget_reported_wait(),
            HookSignal::PromptSubmitted => {
                self.wait_ended();
                self.activity();
            }
            // Reported, not inferred. Applied in `reconsider_pending` rather than here, so
            // that a transcript record folded in afterwards cannot quietly undo it.
            HookSignal::NeedsUser => self.hook_wait = true,
            HookSignal::TurnStopped => {
                self.hook_wait = false;
                // Deliberately not `wait_ended`: the status is set to `idle` two lines
                // down, and a wait that ends because the turn ended does not pass through
                // `working` on the way.
                // A limit is a stop the session did not choose, and it outranks the ordinary
                // one (§8). The turn did end — but saying `idle` here would lose the reason.
                if self.status != Status::Limited {
                    self.enter(Status::Idle, Confidence::High);
                }
            }
            // The half §7.2 cannot read from a process disappearing: this session ended
            // because it was asked to. It leaves the board, and it is not a crash.
            HookSignal::EndedCleanly => {
                self.hook_wait = false;
                self.ended = true;
            }
        }
    }

    /// Something answered the prompt: the wait is over, however it was arrived at.
    ///
    /// Called from the two things §4.1 names as the ways a wait ends — the tool starting, and
    /// the last outstanding call being answered — and it ends an *inferred* wait as well as a
    /// reported one, because both rest on the same fact and that fact has just changed.
    ///
    /// **Silence is not one of the ways.** A hook layer that has gone quiet stops being
    /// consulted (`hooks_live`), but what it last reported stays true until something
    /// contradicts it. A permission prompt can sit far longer than `T_hook_quiet` — one was
    /// measured at 38 minutes and a pending call at 4,737 seconds — and forgetting it because
    /// nothing else happened is the failure §4.1 says costs the most.
    fn wait_ended(&mut self) {
        self.hook_wait = false;
        if self.status == Status::AwaitingUser {
            self.enter(Status::Working, Confidence::High);
        }
    }

    /// Drops a wait the hook layer reported, without touching one that was inferred.
    ///
    /// For the session-start signal, which says only that a session is beginning: anything a
    /// hook reported before it is stale by definition, and this is what stops a helper that
    /// was deleted mid-prompt and restored days later from summoning anyone about a prompt
    /// dealt with long ago. It must go no further — an inferred wait rests on positive
    /// evidence (§4.1 rule 3), and a session starting is not evidence against it.
    fn forget_reported_wait(&mut self) {
        if self.hook_wait {
            self.wait_ended();
        }
    }

    /// Whether the hook layer is currently worth believing for this session.
    ///
    /// Recent silence is not doubt — a session can sit untouched for an hour. Silence longer
    /// than `T_hook_quiet` is: an installed hook that has stopped firing is indistinguishable
    /// from no hook at all, and the board must not keep presenting an exactness it is no
    /// longer earning (FR-52).
    fn hooks_live(&self, at: Timestamp) -> bool {
        self.hook_last
            .is_some_and(|last| at.millis_since(last) <= self.thresholds.hook_quiet_ms)
    }

    // ------------------------------------------------------------ the process died

    fn on_process_gone(
        &mut self,
        at: Timestamp,
        entry_left_behind: Option<bool>,
        editor_alive: Option<bool>,
    ) {
        // Whatever the verdict, nobody is being asked for anything by a session that no
        // longer exists. Not `wait_ended`: the status is about to be decided below,
        // and a vanished session does not pass through `working` on its way out.
        self.hook_wait = false;

        // From `idle`, a process ending is a normal end whatever the registry says.
        if self.status == Status::Idle {
            self.ended = true;
            return;
        }

        // The registry is the primary evidence and it was measured directly: a killed
        // process leaves `sessions/<pid>.json` behind, a clean exit deletes it. Where that
        // evidence exists the verdict rests on it; where it does not, the editor's liveness
        // is a fallback inference, and §7.2 calls it one. The two must not be reported with
        // the same confidence — they are not equally sure, and the board may say so.
        let (terminated, confidence) = match entry_left_behind {
            // Killed under the user's feet: the entry outlived the process.
            Some(true) => (true, Confidence::High),
            // A clean shutdown, which is not a crash.
            Some(false) => (false, Confidence::High),
            // The observer was not watching. In hook mode there is a better answer than the
            // editor's liveness: a session that ended because it was asked to would have said
            // so, and this one did not (§3, the `terminated` row). Where the registry *did*
            // answer, it still decides — that evidence was measured directly (§7.2), while
            // this one is the absence of a report.
            None if self.hooks_live(at) => (true, Confidence::High),
            // A live editor means the session died under it; a dead one means the user
            // closed the window.
            None => (editor_alive.unwrap_or(false), Confidence::Inferred),
        };

        if terminated {
            self.enter(Status::Terminated, confidence);
            self.vanished_since = Some(at);
        } else {
            self.ended = true;
        }
    }

    // ------------------------------------------------------------ pending calls (§4.1)

    /// Decides what an outstanding tool call means, given everything known at `at`.
    ///
    /// The order of the rules is the order of §4.1, and the default is `working`: **elapsed
    /// time alone never produces a wait** ([ADR-0008](../../../docs/adr/0008-detect-blocking-by-whether-the-tool-started.md)).
    fn reconsider_pending(&mut self, at: Timestamp) {
        // Rule 4, and it comes first: "When a hook signal is available, rules 2–3 are skipped
        // entirely; the hook decides." Everything below this block is the inference the hook
        // layer exists to replace.
        if self.hooks_live(at) {
            if self.hook_wait {
                // `limited` outranks even this (§8): a session that cannot continue until
                // its quota resets is not waiting for the user, whatever is on screen.
                if self.status != Status::Limited {
                    self.enter(Status::AwaitingUser, Confidence::High);
                }
            }
            // And if no hook has reported a wait, **nothing is done**. An earlier version
            // pulled the session back to `working` here, on the reasoning that in hook mode
            // the hook is the answer. It is not: "no hook said so" also happens when a
            // `Notification` was dropped — a `notification_type` this build does not know, a
            // line torn by a concurrent append — and an inferred wait rests on positive
            // evidence (§4.1 rule 3) that an absence must not overrule. A wait ends when
            // something ends it, which is what `wait_ended` is called from.
            return;
        }

        if self.pending.is_empty() {
            if self.status == Status::AwaitingUser {
                self.enter(Status::Working, Confidence::High);
            }
            return;
        }
        // A limit or an abnormal stop is not overridden by a pending call.
        if matches!(self.status, Status::Limited | Status::Terminated) {
            return;
        }
        // Rule 1: a mode that never prompts cannot be waiting on a permission prompt.
        if !self.mode_can_prompt {
            return;
        }

        let interactive = self.pending.iter().any(|p| {
            ALWAYS_INTERACTIVE.contains(&p.name.as_str())
                && at.millis_since(p.since) >= self.thresholds.pending_interactive_ms
        });
        if interactive {
            self.enter(Status::AwaitingUser, Confidence::High);
            return;
        }

        // The exemption: while a subagent's own file is growing, its parent's pending call
        // is a running subagent rather than a wait (ADR-0007).
        if self.subagent_growing {
            return;
        }

        let no_process = self.no_process_since.is_some_and(|_| {
            self.pending
                .iter()
                .any(|p| at.millis_since(p.since) >= self.thresholds.pending_probe_ms)
        });
        let outlier = self.pending.iter().any(|p| {
            IN_PROCESS_TOOLS.contains(&p.name.as_str())
                && at.millis_since(p.since) >= self.thresholds.inprocess_outlier_ms
        });

        if no_process || outlier {
            self.enter(Status::AwaitingUser, Confidence::High);
        }
    }

    /// A *vanished* session keeps its slot for `T_terminated_visible` and then clears
    /// itself (FR-59). An abnormal stop whose session is still running has no timer — new
    /// activity is what clears that one.
    fn expire_abnormal_stop(&mut self, at: Timestamp) {
        if self.status != Status::Terminated {
            // Activity cleared it; there is nothing left to time out.
            self.vanished_since = None;
            return;
        }
        // Only a vanished session has a timer, and the instant it vanished is recorded where
        // the vanishing was observed. Starting one here would give the same timer to §7.1's
        // still-running session.
        if let Some(since) = self.vanished_since {
            if at.millis_since(since) >= self.thresholds.terminated_visible_ms {
                self.ended = true;
            }
        }
    }

    fn enter(&mut self, status: Status, confidence: Confidence) {
        self.status = status;
        self.confidence = confidence;
    }
}

// ---------------------------------------------------------------- precedence (§8)

/// Rank within **one session**, when several signals are true at once
/// (`docs/session-state-model.md` §8): lower is stronger.
///
/// ```text
/// limited > awaiting_user > terminated > working > idle > unknown
/// ```
///
/// `limited` outranks everything because acting on any other status would be wasted effort —
/// including `terminated`, since a usage limit is a stop that fixes itself.
const fn within_session_rank(status: Status) -> u8 {
    match status {
        Status::Limited => 0,
        Status::AwaitingUser => 1,
        Status::Terminated => 2,
        Status::Working => 3,
        Status::Idle => 4,
        Status::Unknown => 5,
    }
}

/// Rank for **rolling a group up** to one indicator: lower is more demanding.
///
/// ```text
/// awaiting_user > terminated > limited > idle > unknown > working
/// ```
///
/// **This is a different order from [`within_session_rank`]**, and the specification calls
/// that out itself. The roll-up ranks by *how much does this need me*, so `working` — which
/// needs nothing — sinks to the bottom, while within one session it outranks `idle`.
const fn roll_up_rank(status: Status) -> u8 {
    match status {
        Status::AwaitingUser => 0,
        Status::Terminated => 1,
        Status::Limited => 2,
        Status::Idle => 3,
        Status::Unknown => 4,
        Status::Working => 5,
    }
}

/// The status a session shows when several apply at once.
#[must_use]
pub fn strongest(a: Status, b: Status) -> Status {
    if within_session_rank(a) <= within_session_rank(b) {
        a
    } else {
        b
    }
}

/// The one status a folded group stands for.
///
/// Returns `None` for a group with no sessions, which the board does not draw at all — an
/// empty group is absent rather than blank (FR-06).
#[must_use]
pub fn roll_up(statuses: impl IntoIterator<Item = Status>) -> Option<Status> {
    statuses.into_iter().min_by_key(|s| roll_up_rank(*s))
}
