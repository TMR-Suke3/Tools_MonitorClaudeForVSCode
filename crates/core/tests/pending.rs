//! TC-14, TC-15 … TC-15e, TC-27, TC-28b — deciding what a pending tool call means.
//!
//! `docs/session-state-model.md` §4.1 calls this "the hardest inference in the product", and
//! the shape of these cases is the argument of
//! [ADR-0008](../../../docs/adr/0008-detect-blocking-by-whether-the-tool-started.md): a
//! pending call is equally consistent with a permission prompt on screen and with a tool
//! that simply takes a minute, **and nothing in the transcript separates them**. So the
//! default is `working`, for ever, and only positive evidence moves it.
//!
//! The retired `T_pending_ambiguous` turned any pending call amber after 45 seconds and
//! measured 14 % precision. TC-15 is the case that stops it coming back.
//!
//! Ticks are supplied by the harness. The machine cannot notice time passing on its own
//! (ADR-0019), and these recordings are mostly silence — 44 seconds of an unanswered
//! prompt, 4,737 of a pending call.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::event::{Event, Observation};
use mcv_core::machine::{Confidence, Machine};
use mcv_core::palette::Status;
use mcv_core::time::Timestamp;

/// The two timers these cases turn on, copied from `session-state-model.md` §3 rather than
/// read from the crate.
const T_PENDING_INTERACTIVE_MS: i64 = 1_500;
const T_PENDING_PROBE_MS: i64 = 10_000;

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

// ------------------------------------------------ FR-13, rule 2

/// TC-14 — an always-interactive tool is a wait, after the debounce.
///
/// `AskUserQuestion` exists to ask the user something, so a pending call to it needs no
/// process evidence. The recording holds one pending **2,275 seconds**, and the transcript
/// is empty for the whole wait — which is exactly why the transcript cannot be the only
/// source.
#[test]
fn tc_14_an_always_interactive_tool_is_a_wait() {
    let replay = fixtures::replay_transcript("pending-interactive.jsonl").with_ticks(500);
    let (_, seen) = fold(replay.observations);

    assert!(
        seen.contains(&Status::AwaitingUser),
        "a pending AskUserQuestion must summon the user: {seen:?}",
    );
    let last_summons = seen
        .iter()
        .rposition(|s| *s == Status::AwaitingUser)
        .expect("checked above");
    assert!(
        seen[last_summons + 1..].contains(&Status::Working),
        "the wait ends when the answer lands, with no timeout involved: {seen:?}",
    );
}

/// TC-14, the boundary — the debounce is 1.5 s exactly.
///
/// Asserted at the boundary rather than by dense ticks, because the previous branch's bugs
/// were all "the ordering is right and the value is wrong". A threshold is a value.
#[test]
fn tc_14_the_debounce_boundary_is_exact() {
    let announced = announcement_of("pending-interactive.jsonl", "AskUserQuestion");

    for (offset, expected) in [
        (T_PENDING_INTERACTIVE_MS - 1, Status::Working),
        (T_PENDING_INTERACTIVE_MS, Status::AwaitingUser),
    ] {
        let mut machine = Machine::new();
        for observation in &announced.observations {
            machine.observe(observation);
        }
        let at = Timestamp::from_millis(announced.announced_at.as_millis() + offset);
        machine.observe(&Observation::new(at, Event::Tick));

        assert_eq!(
            machine.status(),
            expected,
            "{offset} ms after the call was announced",
        );
    }
}

// ------------------------------------------------ FR-13, rule 3

/// TC-15 — an ordinary pending call never turns amber on time alone.
///
/// 4,737 seconds of a pending `PowerShell` call, with no process evidence supplied. It stays
/// `working` for the whole of it. This is the case that stops the retired
/// `T_pending_ambiguous` from creeping back in.
#[test]
fn tc_15_an_ordinary_pending_call_never_turns_amber_on_time_alone() {
    let replay = fixtures::replay_transcript("pending-ordinary-long.jsonl").with_ticks(1_000);
    let (_, seen) = fold(replay.observations);

    assert!(
        !seen.contains(&Status::AwaitingUser),
        "elapsed time alone produced a summons: {seen:?}",
    );
}

/// TC-15c — the platform failing to answer is **not** evidence.
///
/// A probe that cannot say whether the tool is running must resolve to the calmer status.
/// "No evidence" and "evidence of blocking" are different things, and only one of them is
/// allowed to summon the user.
#[test]
fn tc_15c_unknown_process_state_is_not_evidence() {
    let base = fixtures::replay_transcript("pending-ordinary-long.jsonl").with_ticks(1_000);
    let mut observations = base.observations;
    // Interleave a probe failure early, while the call is outstanding.
    let announced = announcement_of("pending-ordinary-long.jsonl", "PowerShell");
    let at = Timestamp::from_millis(announced.announced_at.as_millis() + T_PENDING_PROBE_MS * 2);
    let position = observations
        .iter()
        .position(|o| o.at >= at)
        .expect("the recording runs well past the probe window");
    observations.insert(position, Observation::new(at, Event::ProbeUnavailable));

    let (_, seen) = fold(observations);
    assert!(
        !seen.contains(&Status::AwaitingUser),
        "a probe that could not answer was treated as evidence of blocking: {seen:?}",
    );
}

/// TC-15b — a pending call with **no process** is a wait, and stops being one the moment a
/// process appears.
///
/// Both halves come from one recording, taken across a real permission prompt held open on
/// purpose: 44.3 s of an empty process table, then `bash.exe` starting at the moment of
/// approval, then 13.9 s of it running.
#[test]
fn tc_15b_a_pending_call_with_no_process_is_a_wait() {
    let replay = fixtures::replay_stream("prompt-observed.jsonl").with_ticks(1_000);
    let (_, seen) = fold(replay.observations);

    assert_eq!(
        seen,
        vec![Status::Working, Status::AwaitingUser, Status::Working],
        "working while the call is fresh, a summons once nothing is running for it, and \
         working again when the process appears",
    );
}

/// TC-15b, the boundary — the summons waits for `T_pending_probe`.
///
/// The probe reports "nothing running" immediately, at the same instant as the tool-use
/// record. Acting on it at once would report a normally-starting tool as blocked: the
/// measured startup lag for a `Bash` call was 2.3 s.
#[test]
fn tc_15b_the_probe_window_is_observed() {
    let replay = fixtures::replay_stream("prompt-observed.jsonl");
    let announced = replay.observations[0].at;

    for (offset, expected) in [
        (T_PENDING_PROBE_MS - 1, Status::Working),
        (T_PENDING_PROBE_MS, Status::AwaitingUser),
    ] {
        let mut machine = Machine::new();
        // The tool-use record and the probe's answer, both at t = 0 in the recording.
        for observation in replay.observations.iter().take(2) {
            machine.observe(observation);
        }
        machine.observe(&Observation::new(
            Timestamp::from_millis(announced.as_millis() + offset),
            Event::Tick,
        ));
        assert_eq!(
            machine.status(),
            expected,
            "{offset} ms after the call was announced"
        );
    }
}

/// TC-15a — a pending call whose process is running is work.
///
/// The 13.9 s the process ran is the case rule 3 used to get wrong 54 times out of 63.
#[test]
fn tc_15a_a_pending_call_with_its_process_running_is_working() {
    let replay = fixtures::replay_stream("prompt-observed.jsonl");
    let start = replay
        .observations
        .iter()
        .position(|o| o.what == Event::ToolProcessStarted)
        .expect("the recording contains the moment of approval");

    let mut machine = Machine::new();
    // Everything up to and including the process starting, so the call is genuinely pending.
    for observation in replay.observations.iter().take(start + 1) {
        machine.observe(observation);
    }
    assert_eq!(machine.status(), Status::Working);

    // Then the whole time it ran, tick by tick.
    let from = replay.observations[start].at.as_millis();
    for second in 1..=14 {
        machine.observe(&Observation::new(
            Timestamp::from_millis(from + second * 1_000),
            Event::Tick,
        ));
        assert_eq!(
            machine.status(),
            Status::Working,
            "a running tool is work, {second} s in",
        );
    }
}

/// TC-15e — a refused prompt clears itself.
///
/// Refusing writes a `tool_result` carrying an error, which ends the pending call exactly as
/// an approval does. So `awaiting_user` never needs a timeout to escape from, and never
/// strands a session whose prompt the user already dismissed.
#[test]
fn tc_15e_a_refused_prompt_clears_itself() {
    let replay = fixtures::replay_stream("prompt-denied.jsonl").with_ticks(1_000);
    let (machine, seen) = fold(replay.observations);

    assert_eq!(
        seen,
        vec![Status::Working, Status::AwaitingUser, Status::Working],
        "the refusal ends the wait: {seen:?}",
    );
    assert_eq!(machine.status(), Status::Working);
}

/// TC-15d — an in-process tool that is a gross outlier for itself is a wait.
///
/// For a tool that never spawns a process the probe can say nothing, so the evidence is the
/// duration measured against *that tool's own* distribution: `Edit` calls over 45 s ran 48×
/// to 3,239× its median, while `Bash` and `PowerShell` are legitimately slow at the same
/// durations.
///
/// The test plan names a fixture derived from `turn-normal.jsonl`. This uses
/// `pending-interactive.jsonl` instead, because it ends on a **real unanswered `Edit`** —
/// an actual recording of the case beats one derived to order.
#[test]
fn tc_15d_an_in_process_outlier_is_a_wait() {
    let replay = fixtures::replay_transcript("pending-interactive.jsonl");
    let last = replay.observations.last().expect("not empty");
    let edit_is_last = matches!(&last.what, Event::Record { entry, .. }
        if matches!(&entry.record, mcv_core::record::Record::Assistant(m)
            if m.tool_uses().any(|(_, name)| name == "Edit")));
    assert!(edit_is_last, "this recording ends on an unanswered Edit");

    let (machine, seen) = fold(replay.then_ticks(60, 1_000).observations);
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "an Edit pending for a minute is far outside its own distribution: {seen:?}",
    );
    assert_eq!(machine.confidence(), Confidence::High);
}

// ------------------------------------------------ FR-57: subagents

/// TC-27 — a parent with a live subagent stays working.
///
/// From the parent's side a running subagent is an ordinary pending call, and a long one:
/// 6 of 15 measured dispatches ran over 45 s, p90 354 s. The subagent's own file growing is
/// what resolves it ([ADR-0007](../../../docs/adr/0007-detect-subagents-from-their-own-file.md)).
#[test]
fn tc_27_a_parent_with_a_live_subagent_stays_working() {
    let announced = announcement_of("subagent-parent-pending.jsonl", "Agent");
    let replay = fixtures::replay_transcript("subagent-parent-pending.jsonl").with_ticks(1_000);

    // The child's file grows throughout the dispatch, which is what the observer sees.
    let mut observations = replay.observations;
    let mut cursor = announced.announced_at.as_millis();
    let mut growth = Vec::new();
    for _ in 0..350 {
        cursor += 1_000;
        growth.push(Observation::new(
            Timestamp::from_millis(cursor),
            Event::SubagentFileGrew,
        ));
    }
    observations.extend(growth);
    observations.sort_by_key(|o| o.at);

    let (_, seen) = fold(observations);
    assert!(
        !seen.contains(&Status::AwaitingUser),
        "a running subagent was mistaken for a wait: {seen:?}",
    );
}

/// TC-28b — the exemption is not unconditional.
///
/// With no subagent file growing, the same long pending call is still `working` on time
/// alone — but a *tool has no process* event still produces a summons. ADR-0007's exemption
/// must not suppress ADR-0008's evidence.
#[test]
fn tc_28b_the_subagent_exemption_does_not_suppress_evidence() {
    let announced = announcement_of("pending-ordinary-long.jsonl", "PowerShell");
    let mut observations = fixtures::replay_transcript("pending-ordinary-long.jsonl")
        .with_ticks(1_000)
        .observations;

    let at = Timestamp::from_millis(announced.announced_at.as_millis() + 1_000);
    let position = observations
        .iter()
        .position(|o| o.at >= at)
        .expect("the recording runs past this point");
    observations.insert(
        position,
        Observation::new(at, Event::NoProcessForPendingCall),
    );

    let (_, seen) = fold(observations);
    assert!(
        seen.contains(&Status::AwaitingUser),
        "positive evidence that the tool never started must still summon: {seen:?}",
    );
}

// ------------------------------------------------ helper

struct Announcement {
    observations: Vec<Observation>,
    announced_at: Timestamp,
}

/// Everything up to and including the record that announced `tool`, with the instant it was
/// announced at.
fn announcement_of(name: &str, tool: &str) -> Announcement {
    let replay = fixtures::replay_transcript(name);
    let index = replay
        .observations
        .iter()
        .position(|o| match &o.what {
            Event::Record { entry, .. } => match &entry.record {
                mcv_core::record::Record::Assistant(message) => {
                    message.tool_uses().any(|(_, name)| name == tool)
                }
                _ => false,
            },
            _ => false,
        })
        .unwrap_or_else(|| panic!("{name} contains a {tool} call"));

    Announcement {
        announced_at: replay.observations[index].at,
        observations: replay.observations.into_iter().take(index + 1).collect(),
    }
}

/// TC-27, the half that makes the exemption observable.
///
/// A subagent runs **inside** the Claude Code process, so the probe will always report that
/// nothing is running for the parent's `Agent` call. Without the exemption every dispatch
/// longer than the probe window would turn amber — which is the failure
/// [ADR-0007](../../../docs/adr/0007-detect-subagents-from-their-own-file.md) exists to
/// prevent, and which the "no summons appears" form of TC-27 cannot see, because an `Agent`
/// call with no evidence at all stays working anyway.
#[test]
fn tc_27_the_exemption_suppresses_the_probe_for_a_live_subagent() {
    let announced = announcement_of("subagent-parent-pending.jsonl", "Agent");
    let base = announced.announced_at.as_millis();

    for (subagent_growing, expected) in [(true, Status::Working), (false, Status::AwaitingUser)] {
        let mut machine = Machine::new();
        for observation in &announced.observations {
            machine.observe(observation);
        }
        // The probe answers immediately, as it does for anything that never spawns a process.
        machine.observe(&Observation::new(
            Timestamp::from_millis(base + 1_000),
            Event::NoProcessForPendingCall,
        ));
        if subagent_growing {
            machine.observe(&Observation::new(
                Timestamp::from_millis(base + 2_000),
                Event::SubagentFileGrew,
            ));
        }
        machine.observe(&Observation::new(
            Timestamp::from_millis(base + T_PENDING_PROBE_MS + 1_000),
            Event::Tick,
        ));

        assert_eq!(
            machine.status(),
            expected,
            "with the subagent file {}growing",
            if subagent_growing { "" } else { "not " },
        );
    }
}

/// Probe evidence is dropped when the pending set changes, and the summons comes back on the
/// next probe rather than being lost.
///
/// Two calls outstanding at once is not hypothetical — `unknown-record-types.jsonl` announces
/// two `PowerShell` calls before either is answered. The probe's answer carries no tool id
/// (the recorded process events have none), so it is a fact about the set, and once the set
/// changes it is a fact about a different question.
///
/// This is the conservative direction, and the cost is one probe cycle of delay rather than a
/// missed summons.
#[test]
fn probe_evidence_does_not_outlive_the_pending_set() {
    let replay = fixtures::replay_transcript("unknown-record-types.jsonl");
    let outstanding_two = replay
        .observations
        .iter()
        .position(|o| match &o.what {
            Event::Record { entry, .. } => matches!(&entry.record,
                mcv_core::record::Record::Assistant(m)
                    if m.tool_uses().any(|(id, _)| id.ends_with("8e74"))),
            _ => false,
        })
        .expect("the recording announces a second call before the first is answered");
    let answered_one = replay
        .observations
        .iter()
        .position(|o| match &o.what {
            Event::Record { entry, .. } => matches!(&entry.record,
                mcv_core::record::Record::User(m)
                    if m.tool_results().any(|(id, _)| id.ends_with("67e8"))),
            _ => false,
        })
        .expect("the recording answers one of the two");

    let mut machine = Machine::new();
    for observation in replay.observations.iter().take(outstanding_two + 1) {
        machine.observe(observation);
    }
    let base = replay.observations[outstanding_two].at.as_millis();

    // Nothing is running for either call, and the window passes: a summons.
    machine.observe(&Observation::new(
        Timestamp::from_millis(base + 1_000),
        Event::NoProcessForPendingCall,
    ));
    machine.observe(&Observation::new(
        Timestamp::from_millis(base + T_PENDING_PROBE_MS + 1_000),
        Event::Tick,
    ));
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "two blocked calls summon"
    );

    // One is answered. The evidence was about the old set, so it goes with it.
    machine.observe(&replay.observations[answered_one]);
    assert_eq!(
        machine.status(),
        Status::Working,
        "answering one call ends the wait it was part of",
    );
    machine.observe(&Observation::new(
        Timestamp::from_millis(base + T_PENDING_PROBE_MS * 10),
        Event::Tick,
    ));
    assert_eq!(
        machine.status(),
        Status::Working,
        "and the stale evidence does not summon again on its own",
    );

    // The observer keeps probing, so a call that really is blocked is reported again.
    let later = base + T_PENDING_PROBE_MS * 10;
    machine.observe(&Observation::new(
        Timestamp::from_millis(later + 1_000),
        Event::NoProcessForPendingCall,
    ));
    machine.observe(&Observation::new(
        Timestamp::from_millis(later + 2_000),
        Event::Tick,
    ));
    assert_eq!(
        machine.status(),
        Status::AwaitingUser,
        "a fresh probe answer summons again: the cost is a cycle of delay, not a lost summons",
    );
}
