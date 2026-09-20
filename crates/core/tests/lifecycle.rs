//! TC-13, TC-23, TC-24, TC-24b, TC-28 — a session ending, and losing the thread.
//!
//! `docs/session-state-model.md` §7.2 is the specification here, and its central measured
//! fact is that **a killed process leaves its `sessions/<pid>.json` behind while a clean
//! exit deletes it**. That difference is the only local evidence separating a crash from a
//! shutdown, and the board's answer differs completely between them: a crash keeps its slot
//! and shouts, a shutdown simply disappears.
//!
//! Two cautions the specification attaches to that signal are what TC-24b is about. The
//! sweep is not limited to the exiting process — any Claude Code process exiting cleanly
//! deletes *every* dead entry it finds (measured twice: 17 collapsed to 3) — so a file
//! disappearing is not evidence about the session it named.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::discovery::discover;
use mcv_core::event::{Event, Observation};
use mcv_core::machine::{Confidence, Machine};
use mcv_core::palette::Status;
use mcv_core::session::{LiveProcess, RegistryEntry};
use mcv_core::time::Timestamp;

/// From `session-state-model.md` §3, as a literal.
const T_TERMINATED_VISIBLE_MS: i64 = 600_000;

/// A machine driven to `working` by the opening of a real recording.
fn working_session() -> (Machine, Timestamp) {
    let mut machine = Machine::new();
    let mut at = Timestamp::from_millis(0);
    for observation in fixtures::replay_transcript("turn-normal.jsonl").observations {
        at = observation.at;
        machine.observe(&observation);
        if machine.status() == Status::Working {
            break;
        }
    }
    assert_eq!(
        machine.status(),
        Status::Working,
        "the recording opens mid-work"
    );
    (machine, at)
}

// ------------------------------------------------ FR-59

/// TC-23 — a killed session keeps its slot, then clears itself.
#[test]
fn tc_23_a_killed_session_keeps_its_slot() {
    let (mut machine, at) = working_session();

    let change = machine
        .observe(&Observation::new(
            at,
            Event::ProcessGone {
                entry_left_behind: Some(true),
                editor_alive: Some(true),
            },
        ))
        .expect("the death is a transition");
    assert_eq!(
        change.to,
        Status::Terminated,
        "an entry left behind is a process killed under the user's feet",
    );
    assert!(
        !machine.has_ended(),
        "it keeps its slot rather than vanishing"
    );

    // Just short of the timer, it is still there.
    machine.observe(&Observation::new(
        Timestamp::from_millis(at.as_millis() + T_TERMINATED_VISIBLE_MS - 1),
        Event::Tick,
    ));
    assert!(
        !machine.has_ended(),
        "still visible a millisecond short of ten minutes"
    );

    machine.observe(&Observation::new(
        Timestamp::from_millis(at.as_millis() + T_TERMINATED_VISIBLE_MS),
        Event::Tick,
    ));
    assert!(
        machine.has_ended(),
        "after T_terminated_visible it clears itself"
    );
}

/// TC-24 — a clean shutdown is not a crash.
///
/// Three arrangements, all of which must end with the session simply absent: the entry
/// removed with the process, and a death from `idle` whatever the registry did.
#[test]
fn tc_24_a_clean_shutdown_is_not_a_crash() {
    let (mut machine, at) = working_session();
    machine.observe(&Observation::new(
        at,
        Event::ProcessGone {
            entry_left_behind: Some(false),
            editor_alive: Some(true),
        },
    ));
    assert_ne!(
        machine.status(),
        Status::Terminated,
        "a clean exit is not a crash"
    );
    assert!(machine.has_ended());

    for entry_left_behind in [Some(true), Some(false), None] {
        let mut machine = Machine::new();
        for observation in fixtures::replay_transcript("turn-normal.jsonl").observations {
            machine.observe(&observation);
        }
        // The recording ends `working` — drive it to idle first, which is where a normal end
        // happens from.
        let idle_at = Timestamp::from_millis(0);
        let mut idle = Machine::new();
        let mut last = idle_at;
        for observation in fixtures::replay_transcript("turn-normal.jsonl").observations {
            last = observation.at;
            idle.observe(&observation);
            if idle.status() == Status::Idle {
                break;
            }
        }
        assert_eq!(idle.status(), Status::Idle);

        idle.observe(&Observation::new(
            last,
            Event::ProcessGone {
                entry_left_behind,
                editor_alive: Some(true),
            },
        ));
        assert_ne!(
            idle.status(),
            Status::Terminated,
            "a process ending from idle is a normal end, whatever the registry says \
             (entry_left_behind = {entry_left_behind:?})",
        );
        assert!(idle.has_ended());
    }
}

/// TC-24, the unwatched case — with no registry evidence, the editor decides.
///
/// An observer that was not running when the process died cannot know what the registry did.
/// A live editor means the session died under it; a dead editor means the user closed the
/// window, which is not a crash.
#[test]
fn tc_24_without_registry_evidence_the_editor_decides() {
    for (editor_alive, expected_terminated) in [(Some(true), true), (Some(false), false)] {
        let (mut machine, at) = working_session();
        machine.observe(&Observation::new(
            at,
            Event::ProcessGone {
                entry_left_behind: None,
                editor_alive,
            },
        ));
        assert_eq!(
            machine.status() == Status::Terminated,
            expected_terminated,
            "editor_alive = {editor_alive:?}",
        );
    }
}

/// TC-24b — a bulk sweep is not a wave of deaths.
///
/// Claude Code's own garbage collection deletes every dead entry it can find when any
/// process exits cleanly. Discovery must read that as what it is — files about sessions that
/// were already gone — and not as fourteen sessions dying at once.
#[test]
fn tc_24b_a_bulk_sweep_is_not_a_wave_of_deaths() {
    let all: Vec<(bool, RegistryEntry)> = fixtures::registry_entries()
        .into_iter()
        .map(|(live, json)| (live, RegistryEntry::parse(&json).expect("fixture entry")))
        .collect();
    let entries: Vec<RegistryEntry> = all.iter().map(|(_, e)| e.clone()).collect();
    let live: Vec<LiveProcess> = all
        .iter()
        .filter(|(live, _)| *live)
        .map(|(_, e)| LiveProcess {
            pid: e.pid,
            started_unix_seconds: e.proc_start_unix_seconds(),
        })
        .collect();

    let before = discover(&entries, &live);

    // The sweep: every entry whose process is dead disappears, in one tick.
    let swept: Vec<RegistryEntry> = all
        .iter()
        .filter(|(is_live, _)| *is_live)
        .map(|(_, e)| e.clone())
        .collect();
    assert!(
        entries.len() - swept.len() >= 14,
        "the recording's sweep removes fourteen entries, as the measured one did",
    );
    let after = discover(&swept, &live);

    assert_eq!(
        before.workspaces, after.workspaces,
        "the sweep removed only entries whose sessions were already gone; nothing the board \
         was drawing may change",
    );
    assert_eq!(after.stale, 0, "there is nothing stale left to count");
}

// ------------------------------------------------ FR-57

/// TC-28 — a subagent is never a session.
///
/// The side chain is written to its own `agent-*.jsonl` with its own `sessionId`, and has no
/// registry entry at all — so "never shown as its own session" comes for free
/// ([ADR-0007](../../../docs/adr/0007-detect-subagents-from-their-own-file.md)). This case
/// asserts the two halves of that: the recording really is a side chain, and its id really
/// is absent from the registry.
#[test]
fn tc_28_a_subagent_is_never_a_session() {
    let replay = fixtures::replay_transcript("subagent-child.jsonl");
    let mut child_ids: Vec<String> = Vec::new();
    let mut records = 0;

    for observation in &replay.observations {
        if let Event::Record { entry, .. } = &observation.what {
            records += 1;
            assert!(
                entry.meta.is_sidechain,
                "every record in a subagent's own file is a side chain",
            );
            if let Some(id) = &entry.meta.session_id {
                if !child_ids.contains(id) {
                    child_ids.push(id.clone());
                }
            }
        }
    }
    assert!(records > 0);
    assert_eq!(
        child_ids.len(),
        1,
        "a subagent file carries one sessionId of its own"
    );

    let registry: Vec<RegistryEntry> = fixtures::registry_entries()
        .into_iter()
        .map(|(_, json)| RegistryEntry::parse(&json).expect("fixture entry"))
        .collect();
    assert!(
        !registry.iter().any(|e| child_ids.contains(&e.session_id)),
        "a subagent has no registry entry, which is what keeps it off the board",
    );
}

// ------------------------------------------------ FR-10

/// TC-13 — unparseable input gives a neutral status, never a guess and never a vanishing.
///
/// `unknown` means "live, but the observer cannot tell". The session is still running; what
/// has failed is the reading of it, and the board must say which.
#[test]
fn tc_13_unparseable_input_gives_a_neutral_status() {
    let (mut machine, at) = working_session();

    let change = machine
        .observe(&Observation::new(at, Event::Unreadable))
        .expect("losing the thread is a transition");
    assert_eq!(change.to, Status::Unknown);
    assert!(
        !machine.has_ended(),
        "the session is still running; only the reading of it failed",
    );

    // And it recovers: any recognised activity puts the session back on the map.
    let next = fixtures::replay_transcript("turn-normal.jsonl")
        .observations
        .into_iter()
        .next()
        .expect("the recording is not empty");
    machine.observe(&next);
    assert_eq!(
        machine.status(),
        Status::Working,
        "unknown is a gap in the observer's knowledge, not a terminal state",
    );
}

/// The confidence a death is reported with is not the same in both cases, and §7.2 is
/// explicit about why: the registry is **measured evidence** — a killed process leaves
/// `sessions/<pid>.json` behind, a clean exit deletes it — while the editor's liveness is
/// what the specification calls "the old inference", used only when the observer was not
/// watching.
///
/// Reporting both as equally sure would throw away a distinction the data supports.
#[test]
fn a_death_is_reported_with_the_confidence_its_evidence_earns() {
    let (mut watched, at) = working_session();
    let change = watched
        .observe(&Observation::new(
            at,
            Event::ProcessGone {
                entry_left_behind: Some(true),
                editor_alive: Some(true),
            },
        ))
        .expect("a transition");
    assert_eq!(change.to, Status::Terminated);
    assert_eq!(
        change.confidence,
        Confidence::High,
        "an entry left behind is measured evidence, not an inference",
    );

    let (mut unwatched, at) = working_session();
    let change = unwatched
        .observe(&Observation::new(
            at,
            Event::ProcessGone {
                entry_left_behind: None,
                editor_alive: Some(true),
            },
        ))
        .expect("a transition");
    assert_eq!(change.to, Status::Terminated);
    assert_eq!(
        change.confidence,
        Confidence::Inferred,
        "with no registry evidence the verdict rests on a fallback, and must say so",
    );
}

/// The `T_terminated_visible` timer belongs to a **vanished** session only.
///
/// §7.1's abnormal stop — the turn died, the process is alive, the user can type again — has
/// no timer: new activity is what clears it. An earlier version started the timer for any
/// `terminated` status, which would have evicted a live session from the board after ten
/// minutes. Hiding a session that still exists is the opposite of what this board is for.
///
/// Only half of this is reachable from the recordings. The other half needs a turn killed by
/// exhausted retries, and no record with `retryAttempt == maxRetries` exists in the corpus
/// (U-1) — so the guarantee is carried by the shape of the code, where the timer's start
/// instant is recorded at the vanishing and nowhere else, rather than by a case here.
#[test]
fn only_a_vanished_session_times_out() {
    let (mut machine, at) = working_session();
    machine.observe(&Observation::new(
        at,
        Event::ProcessGone {
            entry_left_behind: Some(true),
            editor_alive: Some(true),
        },
    ));
    assert_eq!(machine.status(), Status::Terminated);

    // Ticks alone carry it out.
    machine.observe(&Observation::new(
        Timestamp::from_millis(at.as_millis() + T_TERMINATED_VISIBLE_MS),
        Event::Tick,
    ));
    assert!(machine.has_ended(), "a vanished session clears its slot");
}
