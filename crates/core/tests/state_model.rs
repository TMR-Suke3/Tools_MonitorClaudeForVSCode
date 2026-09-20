//! TC-09 … TC-26 — the status state machine (`docs/session-state-model.md`).
//!
//! Every case is a pure `(ordered events) → (status sequence)` assertion (NFR-11): the
//! recording is folded into a [`Machine`] and the statuses it passes through are compared
//! against what `docs/test-plan.md` §5.2 says they must be.
//!
//! **The thresholds below are literals, copied from `session-state-model.md` §3**, and the
//! assertions compare against those rather than against `Thresholds::default()`. Reading
//! them from the crate would make the suite agree with whatever the code currently demands
//! — the failure that got past two branches of this project already, once on the palette's
//! tier thresholds and once on the FR-02 filter. One test checks the crate's defaults
//! against these literals, and that is the only place they meet.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::event::{Event, Observation};
use mcv_core::machine::{Machine, Thresholds};
use mcv_core::palette::Status;
use mcv_core::time::Timestamp;

// ------------------------------------------------ the timers, from the document

const T_IDLE_FALLBACK_MS: i64 = 3_000;
const T_PENDING_INTERACTIVE_MS: i64 = 1_500;
const T_PENDING_PROBE_MS: i64 = 10_000;
const T_TERMINATED_VISIBLE_MS: i64 = 600_000; // 10 minutes

/// The crate's defaults are the specification's numbers, and this is the one place the two
/// are compared. Everything else in this file uses the literals above.
#[test]
fn the_timers_match_the_document() {
    let t = Thresholds::default();
    assert_eq!(
        t.idle_fallback_ms, T_IDLE_FALLBACK_MS,
        "§3: T_idle_fallback is 3 s"
    );
    assert_eq!(
        t.pending_interactive_ms, T_PENDING_INTERACTIVE_MS,
        "§3: T_pending_interactive is 1.5 s",
    );
    assert_eq!(
        t.pending_probe_ms, T_PENDING_PROBE_MS,
        "§3: T_pending_probe is 10 s — and is the number that table calls the least evidenced",
    );
    assert_eq!(
        t.terminated_visible_ms, T_TERMINATED_VISIBLE_MS,
        "§3: T_terminated_visible is 10 minutes",
    );
}

// ------------------------------------------------ helpers

/// Folds a whole recording in, returning every status the session passed through, starting
/// with the one it began in.
fn statuses(name: &str) -> Vec<Status> {
    let mut machine = Machine::new();
    let mut seen = vec![machine.status()];
    for observation in fixtures::replay_transcript(name).observations {
        if let Some(change) = machine.observe(&observation) {
            seen.push(change.to);
        }
    }
    seen
}

/// Every observation paired with the status the machine held **after** folding it in.
///
/// Cases that name a particular record ("idle at the end_turn record") need this rather than
/// the list of transitions, because they are about *where* in the recording a status holds.
fn trace(name: &str) -> Vec<(Observation, Status)> {
    let mut machine = Machine::new();
    fixtures::replay_transcript(name)
        .observations
        .into_iter()
        .map(|observation| {
            machine.observe(&observation);
            (observation, machine.status())
        })
        .collect()
}

/// Index of the first record for which `wanted` holds.
fn index_of(
    trace: &[(Observation, Status)],
    wanted: impl Fn(&mcv_core::record::Entry) -> bool,
) -> usize {
    trace
        .iter()
        .position(|(o, _)| match &o.what {
            Event::Record { entry, .. } => wanted(entry),
            _ => false,
        })
        .expect("the recording contains the record this case is about")
}

/// The machine after a whole recording, for cases that care about the end state.
fn folded(name: &str) -> Machine {
    let mut machine = Machine::new();
    for observation in fixtures::replay_transcript(name).observations {
        machine.observe(&observation);
    }
    machine
}

// ------------------------------------------------ FR-08

/// TC-09 — exactly one status at every point, and `idle` **at the `end_turn` record**.
///
/// Not "the recording ends idle": this one ends `working`, because a new prompt is queued
/// 12.7 hours after the turn ended and the queue-enqueue is activity. The assertion is about
/// where the status holds, not where the file stops — an earlier version of this case
/// checked the final status and failed against a machine that was behaving correctly.
#[test]
fn tc_09_exactly_one_status_at_every_point() {
    let mut machine = Machine::new();
    let mut changes = 0;
    for observation in fixtures::replay_transcript("turn-normal.jsonl").observations {
        let before = machine.status();
        if let Some(change) = machine.observe(&observation) {
            assert_eq!(
                change.from, before,
                "a transition must report the status it left"
            );
            assert_ne!(
                change.from, change.to,
                "a transition that changes nothing is not one",
            );
            changes += 1;
        } else {
            assert_eq!(machine.status(), before, "no transition means no change");
        }
    }
    assert!(changes > 0, "a whole turn must move the status");

    let trace = trace("turn-normal.jsonl");
    let end_turn = index_of(&trace, |entry| {
        matches!(&entry.record, mcv_core::record::Record::Assistant(m)
            if m.stop_reason == mcv_core::record::StopReason::EndTurn)
    });
    assert_eq!(
        trace[end_turn].1,
        Status::Idle,
        "the turn ends structurally, at its stop reason",
    );

    let queued = index_of(&trace, |entry| {
        matches!(&entry.record, mcv_core::record::Record::QueueOperation(_))
    });
    assert!(
        queued > end_turn,
        "this recording queues a prompt after the turn ended"
    );
    assert_eq!(
        trace[queued].1,
        Status::Working,
        "a queued prompt is activity and wakes the session again",
    );
}

/// TC-10 — a `null` stop reason does **not** end a turn.
///
/// It is a mid-stream partial record: one logical turn is written as several records and
/// only the last carries a reason (`docs/observation-sources.md` §2.3). Treating null as a
/// stop would end turns that are still being written.
#[test]
fn tc_10_a_null_stop_reason_does_not_end_a_turn() {
    let seen = statuses("streaming-null-stop-reason.jsonl");

    let idle_before_the_end = seen[..seen.len() - 1].contains(&Status::Idle);
    assert!(
        !idle_before_the_end,
        "the session went idle mid-stream: {seen:?}",
    );
}

/// TC-11 — `stop_sequence` **does** end a turn.
#[test]
fn tc_11_stop_sequence_ends_a_turn() {
    let seen = statuses("api-error-retrying.jsonl");
    assert!(
        seen.contains(&Status::Idle),
        "a stop_sequence is a stop reason and ends the turn: {seen:?}",
    );
}

/// TC-16 — a short pending call never flickers.
///
/// `turn-normal.jsonl` is full of sub-second tool calls. Not one of them may produce a
/// transition: a board that flickers amber on ordinary work teaches the user to ignore it.
#[test]
fn tc_16_a_short_pending_call_never_flickers() {
    let seen = statuses("turn-normal.jsonl");
    assert!(
        !seen.contains(&Status::AwaitingUser),
        "an ordinary turn produced a summons: {seen:?}",
    );
}

/// TC-17 — `idle` asserts nothing about the work.
///
/// There is deliberately no "finished successfully" status
/// ([ADR-0005](../../../docs/adr/0005-no-goal-achievement-status.md)), so the machine must
/// carry no field that could be rendered as one. Checked at the moment the session goes
/// idle: the only detail any status carries is the usage limit's two instants.
#[test]
fn tc_17_idle_asserts_nothing_about_the_work() {
    let mut machine = Machine::new();
    let mut idled = false;
    for observation in fixtures::replay_transcript("turn-normal.jsonl").observations {
        if let Some(change) = machine.observe(&observation) {
            if change.to == Status::Idle {
                idled = true;
                assert_eq!(
                    change.detail,
                    mcv_core::machine::Detail { limit: None },
                    "`idle` carries no verdict, and there is no field for one to hide in",
                );
            }
        }
    }
    assert!(idled, "the recording contains a turn that ends");
}

/// TC-18 — time-in-status comes from the newest **timestamped** record.
///
/// `tail-untimestamped.jsonl` ends on a `last-prompt`, which carries no timestamp at all.
/// Measuring from "the last line" would give an instant the recording never contained.
#[test]
fn tc_18_elapsed_time_comes_from_the_newest_timestamped_record() {
    let replay = fixtures::replay_transcript("tail-untimestamped.jsonl");
    let stamped: Vec<Timestamp> = replay
        .observations
        .iter()
        .filter_map(|o| match &o.what {
            Event::Record { entry, .. } => entry.meta.at,
            _ => None,
        })
        .collect();

    let last_line_has_no_stamp = match &replay.observations.last().expect("not empty").what {
        Event::Record { entry, .. } => entry.meta.at.is_none(),
        _ => false,
    };
    assert!(
        last_line_has_no_stamp,
        "this recording is kept because it ends on a record with no timestamp",
    );

    let newest = *stamped.iter().max().expect("the recording has timestamps");
    let last_observation = replay.observations.last().expect("not empty").at;
    assert_eq!(
        last_observation, newest,
        "the untimestamped tail is seen at the newest stamp the recording actually carries",
    );
}

// ------------------------------------------------ FR-58: errors

/// TC-20 — a retry in flight is not an abnormal stop.
///
/// Two `api_error` records, retry 1 of 10 and 2 of 10. Retrying *is* activity, and the board
/// must not cry wolf on a hiccup.
#[test]
fn tc_20_a_retry_in_flight_is_not_an_abnormal_stop() {
    let seen = statuses("api-error-retrying.jsonl");
    assert!(
        !seen.contains(&Status::Terminated),
        "a retry that is still in flight ended the session: {seen:?}",
    );
}

/// TC-21 — a stale error does not resurrect an ended turn.
///
/// The `api_error` records are appended **after** the turn ended, carrying older timestamps
/// than the newest record already seen. Ordering by append position is what makes them
/// recognisable as history rather than news
/// ([ADR-0006](../../../docs/adr/0006-order-events-by-append-position.md)).
#[test]
fn tc_21_a_stale_error_does_not_resurrect_an_ended_turn() {
    let seen = statuses("api-error-late-append.jsonl");
    assert!(
        !seen.contains(&Status::Terminated),
        "a late-appended error produced an abnormal stop: {seen:?}",
    );
    assert!(
        !seen.contains(&Status::Limited),
        "nor may it produce a limit: {seen:?}",
    );
}

/// TC-22 — a connection error has a different shape and still parses.
///
/// `error.connection` set, `error.status` absent. The status is unchanged: a transport
/// failure that is being retried is a hiccup like any other.
#[test]
fn tc_22_a_connection_error_has_a_different_shape() {
    let seen = statuses("api-error-connection.jsonl");
    assert!(
        !seen.contains(&Status::Terminated),
        "a connection error is not by itself an abnormal stop: {seen:?}",
    );
}

// ------------------------------------------------ FR-15: the usage limit

/// TC-25 — a rejected quota enters the limit status, from structure alone.
///
/// The fixture carries no message text at all, so a detector that needed the wording could
/// not pass this test.
#[test]
fn tc_25_a_rejected_quota_enters_the_limit_status() {
    let seen = statuses("usage-limit-rejected.jsonl");
    assert!(
        seen.contains(&Status::Limited),
        "a quotaLimits record with status \"rejected\" must produce `limited`: {seen:?}",
    );
}

/// TC-25b — the limit clears with no marker for it.
///
/// Nothing announces recovery. The next successful assistant record ends the status, and
/// replaying past it must leave `limited` behind without any dedicated event.
#[test]
fn tc_25b_the_limit_clears_with_no_marker_for_it() {
    let seen = statuses("usage-limit-rejected.jsonl");
    let limited_at = seen
        .iter()
        .position(|s| *s == Status::Limited)
        .expect("the recording is a session cut off by the limit");

    assert!(
        seen[limited_at + 1..].iter().any(|s| *s != Status::Limited),
        "the recording resumes afterwards, and the status must resume with it: {seen:?}",
    );
    assert_eq!(
        folded("usage-limit-rejected.jsonl").detail().limit,
        None,
        "once the wait is over the board has nothing left to count down",
    );
}

/// TC-25c / TC-25e — the reset time is read, and both ends of the wait are emitted.
///
/// A display cannot derive how long the wait is from the reset instant alone, and only the
/// core sees the transcript the rejection arrived in. Measured gaps ran from 5.4 to 215.3
/// minutes, so a fixed offset from the rejection would be wrong by up to three and a half
/// hours.
#[test]
fn tc_25c_and_e_both_ends_of_the_wait_are_emitted() {
    let mut machine = Machine::new();
    let mut limit = None;
    for observation in fixtures::replay_transcript("usage-limit-rejected.jsonl").observations {
        if let Some(change) = machine.observe(&observation) {
            if change.to == Status::Limited {
                limit = change.detail.limit;
            }
        }
    }

    let limit = limit.expect("the limit transition carries its window");
    let resets_at = limit
        .resets_at
        .expect("the record states the reset time exactly");

    let rejected_at_seconds = limit.rejected_at.as_millis() / 1000;
    assert!(
        resets_at > rejected_at_seconds,
        "the reset ({resets_at}) must be after the rejection ({rejected_at_seconds})",
    );
    let wait_minutes = (resets_at - rejected_at_seconds) as f64 / 60.0;
    assert!(
        (5.0..6.0).contains(&wait_minutes),
        "this recording's wait is 5.7 minutes; got {wait_minutes:.1}",
    );
}

// ------------------------------------------------ FR-56

/// TC-26 — compaction is activity.
///
/// A long session compacting its context is work, not a pause. The window that matters is
/// the one the test plan names: **between the queued prompt and the records that follow the
/// boundary**. The recording does go idle before that, at the `end_turn` of the previous
/// turn, and that idle is correct — an earlier version of this case forbade it and was
/// wrong.
#[test]
fn tc_26_compaction_is_activity() {
    let trace = trace("compaction.jsonl");
    let queued = index_of(&trace, |entry| {
        matches!(&entry.record, mcv_core::record::Record::QueueOperation(_))
    });
    let boundary = index_of(&trace, |entry| {
        matches!(
            &entry.record,
            mcv_core::record::Record::System(
                mcv_core::record::SystemNotice::CompactBoundary { .. }
            )
        )
    });
    assert!(
        boundary > queued,
        "the boundary follows the queued prompt in this recording"
    );

    for (index, (_, status)) in trace.iter().enumerate().skip(queued) {
        assert_ne!(
            *status,
            Status::Idle,
            "the session went idle at record {index}, inside the compaction window",
        );
    }
}

// ------------------------------------------------ NFR-11

/// TC-45, for the machine — replaying a recording twice gives the same answer, and folding
/// it one record at a time gives the same answer as folding it in one go.
#[test]
fn tc_45_folding_is_deterministic() {
    for name in [
        "turn-normal.jsonl",
        "compaction.jsonl",
        "usage-limit-rejected.jsonl",
        "api-error-retrying.jsonl",
    ] {
        assert_eq!(
            statuses(name),
            statuses(name),
            "{name} folded differently twice"
        );
    }
}

/// TC-12 — no network and no model.
///
/// Asserted structurally rather than by watching a socket: `mcv-core`'s entire dependency
/// list is two format libraries, so there is nothing in the crate that *could* open a
/// connection. The test reads the manifest, because that is where the property lives.
#[test]
fn tc_12_the_core_has_no_network_and_no_model() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("the crate's own manifest");
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("the manifest has a dependencies section");

    for forbidden in [
        "reqwest",
        "hyper",
        "ureq",
        "tokio",
        "curl",
        "openai",
        "anthropic",
    ] {
        assert!(
            !deps.contains(forbidden),
            "mcv-core must not depend on {forbidden}: the core is a pure function of a \
             recording, and nothing in it may reach the network",
        );
    }
}

/// The `idle` rule has two halves, and a mutation audit found only one of them tested: a
/// turn's stop reason ends the turn **and there must be no pending tool call**
/// (`session-state-model.md` §3). A recording that ends cleanly cannot show the difference,
/// so this case builds the arrangement from two records of one recording.
#[test]
fn a_turn_that_ends_with_a_call_outstanding_is_not_idle() {
    let replay = fixtures::replay_transcript("turn-normal.jsonl");
    let announced = replay
        .observations
        .iter()
        .find(|o| match &o.what {
            Event::Record { entry, .. } => match &entry.record {
                mcv_core::record::Record::Assistant(m) => m.tool_uses().next().is_some(),
                _ => false,
            },
            _ => false,
        })
        .expect("the recording announces tool calls")
        .clone();
    let ends = replay
        .observations
        .iter()
        .find(|o| match &o.what {
            Event::Record { entry, .. } => matches!(&entry.record,
                mcv_core::record::Record::Assistant(m)
                    if m.stop_reason == mcv_core::record::StopReason::EndTurn),
            _ => false,
        })
        .expect("the recording contains a turn that ends")
        .clone();

    // Both at the same instant, so no timer can fire and mask the rule under test. An
    // earlier version let the records keep their own stamps, which are far enough apart that
    // the in-process outlier rule turned the session amber — and an assertion of merely
    // "not idle" then passed on the wrong status, hiding the mutation it was written for.
    let mut machine = Machine::new();
    machine.observe(&announced);
    machine.observe(&Observation::new(announced.at, ends.what.clone()));
    assert_eq!(
        machine.status(),
        Status::Working,
        "a stop reason with a call still outstanding does not end the turn: the tool is \
         still out, and the session is still working",
    );
}

/// TC-26, the half the window assertion cannot see — compaction **wakes** a session.
///
/// Asserting "never idle inside the window" passes just as well if a compaction boundary is
/// ignored entirely, because the session was already working. This drives the session to
/// idle first, so the boundary has something to do.
#[test]
fn tc_26_a_compaction_boundary_wakes_an_idle_session() {
    let mut machine = Machine::new();
    for observation in fixtures::replay_transcript("turn-normal.jsonl").observations {
        machine.observe(&observation);
        if machine.status() == Status::Idle {
            break;
        }
    }
    assert_eq!(machine.status(), Status::Idle, "driven to a finished turn");

    let boundary = fixtures::replay_transcript("compaction.jsonl")
        .observations
        .into_iter()
        .find(|o| match &o.what {
            Event::Record { entry, .. } => matches!(
                &entry.record,
                mcv_core::record::Record::System(
                    mcv_core::record::SystemNotice::CompactBoundary { .. }
                )
            ),
            _ => false,
        })
        .expect("the recording contains a compaction boundary");

    machine.observe(&boundary);
    assert_eq!(
        machine.status(),
        Status::Working,
        "compaction is work: a session compacting its context is not a session that stopped",
    );
}

/// TC-25e, sharpened — the rejection instant is the **record's own**, not the observer's.
///
/// The two coincide in a replay, which is why a mutation swapping one for the other survived
/// an audit. They do not coincide in life: the observer sees a record some time after it was
/// written, and the board would then count the wait from the wrong end.
#[test]
fn tc_25e_the_rejection_instant_is_the_records_own() {
    let recorded = fixtures::lines("usage-limit-rejected.jsonl")
        .into_iter()
        .find_map(|(_, line)| {
            let value: serde_json::Value = serde_json::from_str(&line).ok()?;
            (value["quotaLimits"]["status"] == "rejected").then(|| {
                Timestamp::parse_iso8601(value["timestamp"].as_str().expect("stamped"))
                    .expect("the recorded format")
            })
        })
        .expect("the recording contains the rejection");

    // The observer is deliberately five minutes behind the recording here. In a plain replay
    // the two clocks coincide, which is why a mutation that read the observer's clock instead
    // of the record's own survived an audit: nothing told them apart.
    const OBSERVER_LAG_MS: i64 = 300_000;
    let mut machine = Machine::new();
    let mut limit = None;
    for observation in fixtures::replay_transcript("usage-limit-rejected.jsonl").observations {
        let lagged = Observation::new(
            Timestamp::from_millis(observation.at.as_millis() + OBSERVER_LAG_MS),
            observation.what.clone(),
        );
        if let Some(change) = machine.observe(&lagged) {
            if change.to == Status::Limited {
                limit = change.detail.limit;
            }
        }
    }

    assert_eq!(
        limit
            .expect("the limit transition carries its window")
            .rejected_at,
        recorded,
        "the wait starts when the rejection was written, not when the board noticed it",
    );
}

/// §8 — the two precedence orders, which are **not** the same one.
///
/// The specification flags the difference itself, and it is the kind of thing that gets
/// quietly unified by someone tidying up: within a session `working` outranks `idle`,
/// because a session doing something is more informative than one that stopped. Rolling a
/// group up, `working` is **last**, because a group whose loudest member is busy is a group
/// asking for nothing.
#[test]
fn the_two_precedence_orders_differ() {
    use mcv_core::machine::{roll_up, strongest};

    // Within one session.
    assert_eq!(
        strongest(Status::Limited, Status::AwaitingUser),
        Status::Limited
    );
    assert_eq!(
        strongest(Status::AwaitingUser, Status::Terminated),
        Status::AwaitingUser
    );
    assert_eq!(
        strongest(Status::Terminated, Status::Working),
        Status::Terminated
    );
    assert_eq!(
        strongest(Status::Working, Status::Idle),
        Status::Working,
        "a session that is doing something outranks one that stopped",
    );

    // Rolling a group up.
    assert_eq!(
        roll_up([Status::Working, Status::Idle]),
        Some(Status::Idle),
        "a group with something finished in it is not reported as busy",
    );
    assert_eq!(
        roll_up([Status::Working, Status::Idle, Status::AwaitingUser]),
        Some(Status::AwaitingUser),
        "the summons wins whatever else is in the group",
    );
    assert_eq!(
        roll_up([Status::Terminated, Status::Limited]),
        Some(Status::Terminated)
    );
    assert_eq!(
        roll_up([Status::Unknown, Status::Working]),
        Some(Status::Unknown)
    );
    assert_eq!(
        roll_up([]),
        None,
        "a group with no sessions is not drawn at all"
    );

    // The orders disagree, and that disagreement is the point.
    assert_ne!(
        strongest(Status::Working, Status::Idle),
        roll_up([Status::Working, Status::Idle]).expect("two statuses"),
        "unifying the two orders would break one of them",
    );
}
