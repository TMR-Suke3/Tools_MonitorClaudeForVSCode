//! TC-82 … TC-92 — the right-hand column of `docs/session-state-model.md` §3.
//!
//! Hook mode is the opt-in accuracy layer. It "changes *how fast and how certainly* a
//! transition is known, never *what the statuses mean*", so every case below lands on a
//! status the inference suite already asserts — the point is **which evidence got it there**,
//! and that §4.1 rule 4 skips rules 2–3 while a hook is talking.
//!
//! **The hook events are supplied by the harness**, as `docs/test-plan.md` §5.2 says
//! non-transcript events must be. They cannot come from a recording: four of the five hook
//! payloads have never been observed (U-5), and the one that was is documented in
//! `observation-sources.md` §5. What that means for these tests is stated honestly — they
//! pin the *rules*, and the *translation* from Claude Code's event names into this vocabulary
//! is the board's, tested in `crates/board/tests/events.rs` and unverified against a real
//! payload for everything but `Stop`.
//!
//! The transcripts underneath are real. `prompt-observed.jsonl` in particular was recorded
//! across a permission prompt held open on purpose, which is what makes "the hook beat the
//! inference to it" a measurable claim rather than an assertion about two synthetic events.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::event::{Event, HookSignal, Observation};
use mcv_core::machine::{Confidence, Machine, Thresholds};
use mcv_core::palette::Status;
use mcv_core::time::Timestamp;

/// The timers these cases turn on, copied from `session-state-model.md` §3 rather than read
/// from the crate — the suite must not agree with whatever the code currently demands.
const T_PENDING_INTERACTIVE_MS: i64 = 1_500;
const T_PENDING_PROBE_MS: i64 = 10_000;
const T_HOOK_QUIET_MS: i64 = 30 * 60 * 1_000;

/// The one place the document's number and the crate's default meet.
#[test]
fn the_quiet_timer_matches_the_document() {
    assert_eq!(
        Thresholds::default().hook_quiet_ms,
        T_HOOK_QUIET_MS,
        "§3: T_hook_quiet is 30 minutes",
    );
    assert_eq!(
        mcv_core::machine::HOOK_QUIET_MS,
        T_HOOK_QUIET_MS,
        "and the constant the board reads is the same one",
    );
}

// ------------------------------------------------ helpers

fn fold(observations: Vec<Observation>) -> (Machine, Vec<Status>) {
    let mut machine = Machine::new();
    let mut seen = vec![machine.status()];
    for observation in observations {
        if let Some(change) = machine.observe(&observation) {
            seen.push(change.to);
        }
    }
    (machine, seen)
}

fn hook(at_ms: i64, signal: HookSignal) -> Observation {
    Observation::new(Timestamp::from_millis(at_ms), Event::Hook(signal))
}

fn tick(at_ms: i64) -> Observation {
    Observation::new(Timestamp::from_millis(at_ms), Event::Tick)
}

/// Ticks from `from` to `to`, so the machine lives through the interval rather than jumping
/// over it. Time only passes for the machine as events (ADR-0019).
fn ticks(from: i64, to: i64, every: i64) -> Vec<Observation> {
    let mut out = Vec::new();
    let mut cursor = from + every;
    while cursor <= to {
        out.push(tick(cursor));
        cursor += every;
    }
    out
}

// ------------------------------------------------ TC-82 … TC-85: the four reported transitions

/// TC-82 (FR-36) — the prompt-submitted event is activity.
///
/// The inference path reaches `working` from a record appearing in the transcript. This
/// reaches it from the session saying a prompt was submitted, which happens first.
#[test]
fn tc_82_the_prompt_submitted_event_starts_a_turn() {
    let mut machine = Machine::new();
    machine.observe(&hook(0, HookSignal::TurnStopped));
    assert_eq!(machine.status(), Status::Idle, "the turn before this one");

    let change = machine
        .observe(&hook(1_000, HookSignal::PromptSubmitted))
        .expect("a submitted prompt wakes an idle session");
    assert_eq!(change.to, Status::Working);
    assert_eq!(
        change.confidence,
        Confidence::High,
        "reported by the session itself, not inferred from a record appearing",
    );
}

/// TC-83 (FR-13) — the notification event is a wait, immediately.
///
/// This is the case the whole hook layer exists for. The recording is the real permission
/// prompt of `prompt-observed.jsonl`: `tool_use`, then 44.3 seconds with **no process** for
/// the pending call. Inference does reach `awaiting_user` there — but only after
/// `T_pending_probe`, and only because the probe found nothing running. The hook says so at
/// once, with neither timer nor probe involved.
#[test]
fn tc_83_the_notification_event_is_a_wait_at_once() {
    // How long after the tool-use record the hook reports the prompt — checked against both
    // debounces rather than merely described, so that raising a timer cannot quietly turn
    // this case into a test of the inference path.
    const REPORTED_AFTER_MS: i64 = 200;
    const {
        assert!(REPORTED_AFTER_MS < T_PENDING_INTERACTIVE_MS);
        assert!(REPORTED_AFTER_MS < T_PENDING_PROBE_MS);
    }

    let recorded = fixtures::replay_stream("prompt-observed.jsonl").observations;
    let announced = recorded
        .iter()
        .find_map(|o| match &o.what {
            Event::Record { .. } => Some(o.at),
            _ => None,
        })
        .expect("the recording opens with the tool_use record");

    // The transcript record, then the notification a moment later — and nothing else. No
    // probe result, and far less than `T_pending_probe` of elapsed time.
    let mut observations = vec![recorded[0].clone()];
    observations.push(Observation::new(
        Timestamp::from_millis(announced.as_millis() + REPORTED_AFTER_MS),
        Event::Hook(HookSignal::NeedsUser),
    ));

    let (machine, seen) = fold(observations);
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "the session reported the prompt; nothing had to be inferred: {seen:?}",
    );
    assert_eq!(machine.confidence(), Confidence::High);
}

/// TC-84 (FR-13) — a notification that is not a permission prompt never reaches the core.
///
/// Claude Code also notifies when a session has simply been sitting idle. That is not a
/// blocking UI, and turning it amber would be inventing urgency (§9). The vocabulary is what
/// enforces it: there is no variant for it to arrive as, so the board drops it and this test
/// pins the shape of the enum rather than a branch in the machine.
#[test]
fn tc_84_only_a_permission_prompt_can_summon() {
    let signals = [
        HookSignal::Started,
        HookSignal::PromptSubmitted,
        HookSignal::NeedsUser,
        HookSignal::TurnStopped,
        HookSignal::EndedCleanly,
    ];
    let summons: Vec<&HookSignal> = signals
        .iter()
        .filter(|signal| {
            let mut machine = Machine::new();
            machine.observe(&hook(0, **signal));
            machine.status() == Status::AwaitingUser
        })
        .collect();

    assert_eq!(
        summons.len(),
        1,
        "exactly one signal may summon the user, and it is the permission prompt: {summons:?}",
    );
    assert_eq!(summons[0], &HookSignal::NeedsUser);
}

/// TC-85 (FR-08) — the turn-stop event ends the turn.
///
/// `idle` is the one transition inference is already good at: a stop reason is structural,
/// read rather than waited for. The hook says the same thing sooner, and — the part that
/// matters — it says it without the transcript having to be readable.
#[test]
fn tc_85_the_turn_stop_event_ends_the_turn() {
    let mut machine = Machine::new();
    machine.observe(&hook(0, HookSignal::PromptSubmitted));
    assert_eq!(machine.status(), Status::Working);

    let change = machine
        .observe(&hook(5_000, HookSignal::TurnStopped))
        .expect("the turn ended");
    assert_eq!(change.to, Status::Idle);
    assert_eq!(change.confidence, Confidence::High);
}

// ------------------------------------------------ TC-86, TC-87: how a session ends

/// TC-86 (FR-59) — a clean session end is *absent*, never `terminated`.
///
/// §7.2's whole difficulty is that a process disappearing looks the same either way, and the
/// registry's leftover entry is the only local evidence separating the two. A session that
/// says it is ending removes the guess.
#[test]
fn tc_86_a_clean_session_end_is_not_a_crash() {
    let mut machine = Machine::new();
    machine.observe(&hook(0, HookSignal::PromptSubmitted));
    machine.observe(&hook(1_000, HookSignal::EndedCleanly));

    assert!(machine.has_ended(), "the session leaves the board");
    assert_ne!(
        machine.status(),
        Status::Terminated,
        "it ended because it was asked to; nothing was killed",
    );
}

/// TC-87 (FR-59) — the process vanished and nothing reported an ending.
///
/// The registry evidence is missing — the observer was not watching at the moment of death,
/// which §7.2 says leaves it inferring from the editor's liveness. In hook mode there is a
/// better answer: a session that ended cleanly would have said so. Compare the two paths on
/// the *same* input, because the difference is the point.
#[test]
fn tc_87_a_vanished_session_with_no_clean_ending_is_terminated() {
    let gone = |at: i64| {
        Observation::new(
            Timestamp::from_millis(at),
            Event::ProcessGone {
                entry_left_behind: None,
                editor_alive: Some(false),
            },
        )
    };

    // Inference: the editor is gone too, so the user closed the window (§7.2's last-but-one
    // row). Not a crash.
    let mut inferred = Machine::new();
    inferred.observe(&gone(1_000));
    assert!(inferred.has_ended());
    assert_ne!(inferred.status(), Status::Terminated);

    // Hook mode: the same disappearance, but hooks were reporting and none of them said the
    // session was ending.
    let mut hooked = Machine::new();
    hooked.observe(&hook(0, HookSignal::PromptSubmitted));
    hooked.observe(&gone(1_000));
    assert_eq!(
        hooked.status(),
        Status::Terminated,
        "no clean session-end event arrived, so the session did not end cleanly",
    );
    assert_eq!(hooked.confidence(), Confidence::High);
    assert!(
        !hooked.has_ended(),
        "and it keeps its slot for T_terminated_visible (FR-59)",
    );
}

/// The registry still outranks the hook layer's *silence*.
///
/// A hook that did not fire is the absence of a report, while `sessions/<pid>.json` being
/// swept with the process was measured directly (§7.2) — so where the registry answered, it
/// answers. The two only meet when the observer was not watching.
#[test]
fn measured_registry_evidence_beats_a_missing_report() {
    let mut machine = Machine::new();
    machine.observe(&hook(0, HookSignal::PromptSubmitted));
    machine.observe(&Observation::new(
        Timestamp::from_millis(1_000),
        Event::ProcessGone {
            entry_left_behind: Some(false),
            editor_alive: Some(true),
        },
    ));

    assert!(machine.has_ended());
    assert_ne!(
        machine.status(),
        Status::Terminated,
        "the entry went with the process, which is a clean exit however quiet the hooks were",
    );
}

// ------------------------------------------------ TC-88, TC-89: rule 4 and the fallback

/// TC-89 (FR-13) — while a hook is talking, rules 2–3 do not run.
///
/// §4.1 rule 4: *"When a hook signal is available, rules 2–3 are skipped entirely; the hook
/// decides."* The recording is the one that proves rule 3 works — 44.3 seconds of an empty
/// process table for a pending call, which inference turns amber after `T_pending_probe`
/// (TC-15b). With the hook layer alive and saying nothing about a prompt, it must not.
#[test]
fn tc_89_a_live_hook_layer_replaces_the_inference() {
    let recorded = fixtures::replay_stream("prompt-observed.jsonl")
        .with_ticks(500)
        .observations;
    let start = recorded[0].at.as_millis();

    // Inference alone: this is TC-15b, and it summons.
    let (_, inferred) = fold(recorded.clone());
    assert!(
        inferred.contains(&Status::AwaitingUser),
        "the recording is the one that proves rule 3: {inferred:?}",
    );

    // The same recording, with a session-start hook in front of it. Nothing reported a
    // prompt, so nothing is amber — the probe's evidence is not consulted at all.
    let mut with_hook = vec![Observation::new(
        Timestamp::from_millis(start - 1),
        Event::Hook(HookSignal::Started),
    )];
    with_hook.extend(recorded);
    let (_, seen) = fold(with_hook);
    assert!(
        !seen.contains(&Status::AwaitingUser),
        "rule 4 skips rules 2 and 3 entirely; the hook decides, and it said nothing: {seen:?}",
    );
}

/// TC-88 (FR-52) — hooks that stop arriving stop being believed.
///
/// "It **MUST NOT** keep presenting exact-mode confidence it is no longer earning." The same
/// pending call as above: silent for less than `T_hook_quiet` it is still the hook's to
/// decide, and once the silence passes that, rule 3's evidence applies again.
#[test]
fn tc_88_silence_falls_back_to_inference() {
    let probe =
        |at: i64| Observation::new(Timestamp::from_millis(at), Event::NoProcessForPendingCall);

    let announce = fixtures::replay_stream("prompt-observed.jsonl").observations[0].clone();
    let start = announce.at.as_millis();

    let run = |quiet_for: i64| {
        let mut observations = vec![
            Observation::new(
                Timestamp::from_millis(start - 1),
                Event::Hook(HookSignal::Started),
            ),
            announce.clone(),
            probe(start + 1_000),
        ];
        observations.extend(ticks(start, start + quiet_for, 60_000));
        fold(observations).0
    };

    assert_eq!(
        run(T_HOOK_QUIET_MS - 60_000).status(),
        Status::Working,
        "still within the quiet allowance: the hook layer is alive and says nothing is waiting",
    );
    assert_eq!(
        run(T_HOOK_QUIET_MS + 60_000).status(),
        Status::AwaitingUser,
        "the hooks went silent, so the pending call is inference's business again (FR-52)",
    );
}

/// A wait a hook reported long ago does not come back with the hooks.
///
/// A session whose helper was deleted mid-prompt, answered by hand, and whose helper was
/// later restored must not be summoned again for a prompt that was dealt with half an hour
/// ago. What clears it is the **session-start signal** — a session that is starting has
/// nothing outstanding — and not the silence itself, which is the distinction TC-98 turns on.
#[test]
fn a_stale_reported_wait_does_not_survive_the_fallback() {
    let mut observations = vec![hook(0, HookSignal::NeedsUser)];
    observations.extend(ticks(0, T_HOOK_QUIET_MS + 60_000, 60_000));
    observations.push(hook(T_HOOK_QUIET_MS + 120_000, HookSignal::Started));

    let (machine, seen) = fold(observations);
    assert_ne!(
        machine.status(),
        Status::AwaitingUser,
        "the hooks came back, but the prompt they reported is long gone: {seen:?}",
    );
}

/// TC-98 (FR-13) — a wait outlives the hooks going quiet.
///
/// The failure this exists for: the board forgetting a permission prompt that is still on
/// screen, because nothing else happened for half an hour. Prompts do sit that long — one was
/// measured at 38 minutes and a pending call at 4,737 seconds — and `T_hook_quiet` is 30.
///
/// Falling back to inference means the hook layer stops being *consulted*. It does not mean
/// what it last reported becomes false: a wait ends when something ends it (TC-92), and
/// silence is not one of those things.
#[test]
fn tc_98_a_reported_wait_outlives_the_silence() {
    let announce = fixtures::replay_stream("prompt-observed.jsonl").observations[0].clone();
    let start = announce.at.as_millis();

    let mut observations = vec![
        announce,
        Observation::new(
            Timestamp::from_millis(start),
            Event::Hook(HookSignal::NeedsUser),
        ),
    ];
    // Nothing answers it, and nothing else is reported, for well past the quiet allowance.
    observations.extend(ticks(start, start + T_HOOK_QUIET_MS + 300_000, 60_000));

    let (machine, seen) = fold(observations);
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "the prompt is still on screen; the hooks going quiet is not an answer to it: {seen:?}",
    );
}

/// TC-99 (FR-13) — a live hook layer does not cancel an inferred wait.
///
/// `awaiting_user` reached through §4.1 rule 3 rests on positive evidence: a pending call
/// with nothing running for it. Hooks arriving afterwards mean rules 2–3 stop being consulted
/// — they do not mean the evidence was wrong. "No hook reported a prompt" also happens when a
/// `Notification` carried a `notification_type` this build does not know, or when a line was
/// torn by a concurrent append, and neither of those is a session that stopped waiting.
#[test]
fn tc_99_hooks_arriving_do_not_cancel_an_inferred_wait() {
    let recorded = fixtures::replay_stream("prompt-observed.jsonl")
        .with_ticks(500)
        .observations;

    // Fold until inference has summoned the user — this is TC-15b's path, no hooks involved.
    let mut machine = Machine::new();
    let mut summoned_at = None;
    for (i, observation) in recorded.iter().enumerate() {
        machine.observe(observation);
        if machine.status() == Status::AwaitingUser {
            summoned_at = Some(i);
            break;
        }
    }
    let summoned_at = summoned_at.expect("the recording summons the user through inference");
    let at = recorded[summoned_at].at.as_millis();

    // Now the hook layer starts reporting — about the session, but not about the prompt.
    machine.observe(&hook(at + 100, HookSignal::Started));
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "a hook layer that says nothing about a prompt has not said the prompt is over",
    );

    machine.observe(&tick(at + 5_000));
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "and it does not drift out of it on a tick either",
    );
}

// ------------------------------------------------ TC-90 … TC-92: the awkward corners

/// TC-90 (FR-15) — a usage limit outranks anything a hook reports.
///
/// §8: `limited > awaiting_user`. The transcript stays the source for the limit — "hooks add
/// nothing here" — and a session that cannot continue until its quota resets is not waiting
/// for the user, whatever is on screen.
#[test]
fn tc_90_a_limit_outranks_a_reported_wait() {
    // Up to the rejection and no further. The recording continues into the session's
    // recovery — TC-25b's point, that nothing announces the end of a limit — and replaying
    // past it would be asking about a session that is no longer limited.
    let mut machine = Machine::new();
    let mut at = Timestamp::from_millis(0);
    for observation in fixtures::replay_transcript("usage-limit-rejected.jsonl").observations {
        machine.observe(&observation);
        at = observation.at;
        if machine.status() == Status::Limited {
            break;
        }
    }
    assert_eq!(
        machine.status(),
        Status::Limited,
        "the recording must reach the rejection for this case to mean anything",
    );

    let reported = machine.observe(&Observation::new(
        Timestamp::from_millis(at.as_millis() + 1_000),
        Event::Hook(HookSignal::NeedsUser),
    ));
    assert_eq!(
        reported, None,
        "acting on any other status would be wasted effort (§8): a session that cannot          continue until its quota resets is not waiting for the user",
    );

    let stopped = machine.observe(&Observation::new(
        Timestamp::from_millis(at.as_millis() + 2_000),
        Event::Hook(HookSignal::TurnStopped),
    ));
    assert_eq!(
        stopped, None,
        "and the turn ending does not downgrade the reason it ended",
    );
    assert_eq!(machine.status(), Status::Limited);
    assert!(
        machine.detail().limit.is_some(),
        "the reset instant survives, because the countdown is what the status is for",
    );
}

/// TC-91 (NFR-11) — the answer does not depend on which arrived first.
///
/// Hook lines and transcript lines are read in the same poll and there is no honest order
/// between them: a transcript line carries no arrival time, and the hook's own stamp comes
/// from a different process. So the machine is built not to care
/// ([ADR-0020](../../../docs/adr/0020-hook-signals-are-facts-not-status-writes.md)), and this
/// is the test that would fail if a hook signal were ever written straight to the status.
#[test]
fn tc_91_a_hook_and_a_record_in_either_order_agree() {
    let announce = fixtures::replay_stream("prompt-observed.jsonl").observations[0].clone();
    let at = announce.at.as_millis();
    let reported = Observation::new(
        Timestamp::from_millis(at),
        Event::Hook(HookSignal::NeedsUser),
    );

    let (hook_first, _) = fold(vec![reported.clone(), announce.clone()]);
    let (record_first, _) = fold(vec![announce, reported]);

    assert_eq!(
        hook_first.status(),
        record_first.status(),
        "the tool-use record calls itself activity; the hook says the user is being asked. \
         Whichever is folded in first, the session is waiting",
    );
    assert_eq!(hook_first.status(), Status::AwaitingUser);
}

/// TC-92 (FR-13) — a reported wait ends the same two ways an inferred one does.
///
/// §4.1: *"However the user answers, the wait ends by itself."* Approving starts the tool,
/// which is visible as a process; refusing writes a `tool_result` carrying an error. Both
/// recordings are real, and neither involves a timeout.
///
/// The approval half is asserted **at the moment the process appears**, not at the end of the
/// recording. At the end the `tool_result` has landed and would have cleared the wait anyway —
/// so an end-state assertion passes whether or not a running tool ends a reported wait, and
/// the 13.9 s of wrongly-amber board in between goes unmeasured. (It did: the mutation audit
/// walked straight through the first version of this case.)
#[test]
fn tc_92_a_reported_wait_ends_when_the_prompt_is_answered() {
    // Approved: `prompt-observed.jsonl` holds 44.3 s of an empty process table, then the
    // process starts at the moment of approval and runs for 13.9 s.
    let mut approved = fixtures::replay_stream("prompt-observed.jsonl")
        .with_ticks(500)
        .observations;
    let at = approved[0].at.as_millis();
    approved.insert(
        1,
        Observation::new(
            Timestamp::from_millis(at),
            Event::Hook(HookSignal::NeedsUser),
        ),
    );

    let mut machine = Machine::new();
    let mut summoned = false;
    let mut cleared_by_the_process = None;
    let mut answered = false;
    for observation in &approved {
        machine.observe(observation);
        match &observation.what {
            Event::Hook(HookSignal::NeedsUser) => {
                assert_eq!(machine.status(), Status::AwaitingUser);
                summoned = true;
            }
            // The moment of approval. The call is still outstanding — its `tool_result` is
            // 13.9 s away — so nothing but the process starting can have ended the wait.
            Event::ToolProcessStarted => {
                cleared_by_the_process = Some(machine.status());
            }
            Event::Record { entry, .. } => {
                if let mcv_core::record::Record::User(message) = &entry.record {
                    answered |= message.tool_results().count() > 0;
                }
            }
            _ => {}
        }
    }

    assert!(summoned, "the hook reported the prompt");
    assert_eq!(
        cleared_by_the_process,
        Some(Status::Working),
        "the tool is running, so the prompt was answered. Waiting for the `tool_result`          instead would leave the board amber for as long as the tool takes to run",
    );
    assert!(
        answered,
        "the recording must end with the call answered, or the case above proves nothing          about what the process event did",
    );
    assert_eq!(machine.status(), Status::Working);

    // Refused: `prompt-denied.jsonl` ends on a `tool_result` carrying `is_error`, and no
    // process ever starts — so the errored result is the only thing that can end this one.
    let mut denied = fixtures::replay_stream("prompt-denied.jsonl")
        .with_ticks(500)
        .observations;
    let at = denied[0].at.as_millis();
    denied.insert(
        1,
        Observation::new(
            Timestamp::from_millis(at),
            Event::Hook(HookSignal::NeedsUser),
        ),
    );
    assert!(
        !denied
            .iter()
            .any(|o| matches!(o.what, Event::ToolProcessStarted)),
        "the refused recording must contain no process, or this proves nothing",
    );
    let (machine, seen) = fold(denied);
    assert!(seen.contains(&Status::AwaitingUser), "{seen:?}");
    assert_ne!(
        machine.status(),
        Status::AwaitingUser,
        "a refusal ends the wait exactly as an approval does (TC-15e), reported or inferred",
    );
}

/// A second call outstanding does not answer the first one's prompt.
///
/// Two pending calls at once is not hypothetical: `unknown-record-types.jsonl` announces two
/// and answers them one at a time. A `tool_result` for one says nothing about the other — and
/// the other is the one a hook reported a prompt for, so the summons must survive it.
#[test]
fn answering_one_of_two_calls_does_not_end_the_wait() {
    use mcv_core::record::Record;

    let mut machine = Machine::new();
    let mut outstanding = 0usize;
    let mut reported = false;
    let mut checked_partial = false;

    for observation in fixtures::replay_transcript("unknown-record-types.jsonl").observations {
        machine.observe(&observation);
        let Event::Record { entry, .. } = &observation.what else {
            continue;
        };
        match &entry.record {
            Record::Assistant(message) => {
                outstanding += message.tool_uses().count();
                // The moment two are outstanding, a hook reports a prompt. Which of the two
                // it is about is not in the payload, and that is the point.
                if outstanding >= 2 && !reported {
                    machine.observe(&Observation::new(
                        observation.at,
                        Event::Hook(HookSignal::NeedsUser),
                    ));
                    assert_eq!(machine.status(), Status::AwaitingUser);
                    reported = true;
                }
            }
            Record::User(message) => {
                outstanding = outstanding.saturating_sub(message.tool_results().count());
                if reported && outstanding > 0 {
                    assert_eq!(
                        machine.status(),
                        Status::AwaitingUser,
                        "one call answered, one still outstanding: the prompt stands",
                    );
                    checked_partial = true;
                } else if reported {
                    assert_ne!(
                        machine.status(),
                        Status::AwaitingUser,
                        "nothing is outstanding any more, so nothing is being asked",
                    );
                }
            }
            _ => {}
        }
    }

    assert!(
        reported,
        "the recording never had two calls outstanding at once"
    );
    assert!(
        checked_partial,
        "the recording never answered one of two, so nothing was tested",
    );
}
