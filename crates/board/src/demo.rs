//! A board built from the recordings, for looking at.
//!
//! The window has to be photographable to be reviewed, and a board wired to whatever
//! sessions happen to be running is not: two screenshots of it differ for reasons that have
//! nothing to do with the change being reviewed. So this builds a **fixed** board, and it
//! builds it from `tests/fixtures/` rather than from invented data — every status here was
//! derived by folding a real recording through the real state machine.
//!
//! It is a development aid, not a product feature: `mcv-board --demo`.

use std::path::PathBuf;

use mcv_core::event::{Event, Observation};
use mcv_core::machine::Machine;
use mcv_core::palette::{Status, Theme};
use mcv_core::record::parse_line;
use mcv_core::time::Timestamp;

use crate::settings::Settings;
use crate::view::{BoardView, GroupState, Inspect, SessionState, build};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Folds a recording and returns the status it reaches, stopping early if `until` is seen.
///
/// Stopping early is how the mid-recording statuses are reached: `pending-interactive`
/// ends `working` because the question was eventually answered, but for 2,275 seconds
/// before that it was a summons, and the summons is what a board is for.
fn status_of(name: &str, until: Option<Status>) -> Status {
    let path = fixtures().join(name);
    // The recordings live in the repository, found through CARGO_MANIFEST_DIR, so a binary
    // run from anywhere else will not have them. That must not stop the window opening: a
    // board that panics at startup because a development fixture is missing is the worst
    // possible way to say so.
    //
    // `unknown` is the honest answer — live, but the observer cannot tell — which is exactly
    // what has happened.
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!(
            "mcv-board: no recording at {} — the demonstration board needs the repository's tests/fixtures, and is showing `unknown` instead",
            path.display()
        );
        return Status::Unknown;
    };

    let mut machine = Machine::new();
    let mut at = Timestamp::from_millis(0);
    let mut offset = 0u64;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if !trimmed.is_empty() {
            if let Ok(entry) = parse_line(trimmed, offset) {
                if let Some(stamp) = entry.meta.at {
                    at = stamp;
                }
                machine.observe(&Observation::new(
                    at,
                    Event::Record {
                        offset,
                        entry: Box::new(entry),
                    },
                ));
                // A tick five seconds after each record, so that the debounces the board is
                // about actually elapse. Ticking at the record's own instant advances no
                // time at all, and a replay that never lets time pass never reaches
                // `awaiting_user` — the status this board exists for.
                //
                // Five seconds clears T_pending_interactive (1.5 s) without reaching
                // T_pending_probe (10 s) or the outlier threshold (45 s), so the statuses
                // here are the ones the rules give, not ones the replay manufactured.
                machine.observe(&Observation::new(
                    Timestamp::from_millis(at.as_millis() + 5_000),
                    Event::Tick,
                ));
                if until.is_some_and(|target| machine.status() == target) {
                    return machine.status();
                }
            }
        }
        offset += line.len() as u64;
    }
    machine.status()
}

/// The demonstration board: four workspaces, every status the board can draw.
#[must_use]
pub fn board(theme: Theme, inspect: Inspect, settings: &Settings) -> BoardView {
    let working = status_of("turn-normal.jsonl", Some(Status::Working));
    let awaiting = status_of("pending-interactive.jsonl", Some(Status::AwaitingUser));
    let limited = status_of("usage-limit-rejected.jsonl", Some(Status::Limited));
    let idle = status_of("turn-normal.jsonl", Some(Status::Idle));

    let groups = vec![
        GroupState {
            key: "C:/work/sample-repo",
            label: "sample-repo",
            folded: settings.is_folded("C:/work/sample-repo"),
            sessions: vec![
                SessionState {
                    id: "s1",
                    status: awaiting,
                    ai_title: Some("権限プロンプトの応答待ち"),
                    registry_name: None,
                    prompt_line: None,
                    derived_name: None,
                    time: "38m".to_owned(),
                },
                SessionState {
                    id: "s2",
                    status: working,
                    ai_title: Some("fix the transcript tail offsets"),
                    registry_name: None,
                    prompt_line: None,
                    derived_name: None,
                    time: "12s".to_owned(),
                },
                SessionState {
                    id: "s3",
                    status: idle,
                    ai_title: Some("palette thresholds, done"),
                    registry_name: None,
                    prompt_line: None,
                    derived_name: None,
                    time: "2h".to_owned(),
                },
            ],
        },
        GroupState {
            key: "C:/work/other-repo",
            label: "other-repo",
            // Folded on a first run, so the demonstration shows both states; after that
            // the user's own choice wins, like any other group.
            folded: settings.folded.is_empty() || settings.is_folded("C:/work/other-repo"),
            sessions: vec![
                SessionState {
                    id: "s4",
                    status: limited,
                    ai_title: Some("五時間の上限に達した"),
                    registry_name: None,
                    prompt_line: None,
                    derived_name: None,
                    // `limited` shows the reset clock time, not an elapsed count (FR-24).
                    time: "05:50".to_owned(),
                },
                SessionState {
                    id: "s5",
                    status: working,
                    ai_title: Some("regenerate the fixtures"),
                    registry_name: None,
                    prompt_line: None,
                    derived_name: None,
                    time: "4m".to_owned(),
                },
            ],
        },
        GroupState {
            key: "C:/work/third-repo",
            label: "third-repo",
            folded: settings.is_folded("C:/work/third-repo"),
            sessions: vec![
                SessionState {
                    id: "s6",
                    status: Status::Terminated,
                    ai_title: Some("killed while writing the state machine"),
                    registry_name: None,
                    prompt_line: None,
                    derived_name: None,
                    time: "1m".to_owned(),
                },
                // The one session in the recordings with no title of its own: it falls
                // back to the registry's slug, which is what an untitled session shows once
                // the opening prompt is gone too (ADR-0030).
                SessionState {
                    id: "s7",
                    status: Status::Unknown,
                    ai_title: None,
                    registry_name: None,
                    prompt_line: None,
                    derived_name: Some("third-repo-07"),
                    time: "9s".to_owned(),
                },
            ],
        },
        GroupState {
            key: "C:/work/monitor-claude",
            label: "monitor-claude",
            folded: settings.is_folded("C:/work/monitor-claude"),
            sessions: vec![SessionState {
                id: "s8",
                status: idle,
                ai_title: Some("board geometry from ui-overlay §2"),
                registry_name: None,
                prompt_line: None,
                derived_name: None,
                time: "17m".to_owned(),
            }],
        },
    ];

    build(&groups, theme, inspect)
}
