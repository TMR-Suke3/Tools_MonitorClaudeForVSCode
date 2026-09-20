//! TC-01 … TC-08 — discovery, from the recorded registry (FR-01 … FR-07, FR-40).
//!
//! Every case runs on `tests/fixtures/session-registry.json`, which reproduces the measured
//! shape and the measured live/stale ratio: **3 live of 17**.
//!
//! Liveness is supplied as data, not read from this machine's process table
//! ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)),
//! which is what lets a test say "these two entries are running" and get a deterministic
//! answer. The fixture's own `_note_live` marks the recorded ratio; where a case needs a
//! different arrangement — two live sessions in one folder, say — it declares one, because
//! the arrangement is the input under test rather than part of the recording.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::discovery::{Discovery, discover, workspace_label};
use mcv_core::session::{LiveProcess, RegistryEntry};

/// Every entry in the fixture, in file order, with the recording's own liveness flag.
fn entries() -> Vec<(bool, RegistryEntry)> {
    fixtures::registry_entries()
        .into_iter()
        .map(|(live, json)| {
            let entry = RegistryEntry::parse(&json)
                .unwrap_or_else(|e| panic!("fixture entry does not parse: {e:?}"));
            (live, entry)
        })
        .collect()
}

/// The process table as it was when the recording was taken.
fn as_recorded(all: &[(bool, RegistryEntry)]) -> Vec<LiveProcess> {
    all.iter()
        .filter(|(live, _)| *live)
        .map(|(_, e)| LiveProcess {
            pid: e.pid,
            started_unix_seconds: e.proc_start_unix_seconds(),
        })
        .collect()
}

/// A process table in which exactly the named pids are running, with matching stamps.
fn running(all: &[(bool, RegistryEntry)], pids: &[u32]) -> Vec<LiveProcess> {
    all.iter()
        .filter(|(_, e)| pids.contains(&e.pid))
        .map(|(_, e)| LiveProcess {
            pid: e.pid,
            started_unix_seconds: e.proc_start_unix_seconds(),
        })
        .collect()
}

fn just_entries(all: &[(bool, RegistryEntry)]) -> Vec<RegistryEntry> {
    all.iter().map(|(_, e)| e.clone()).collect()
}

fn recorded() -> Discovery {
    let all = entries();
    discover(&just_entries(&all), &as_recorded(&all))
}

// ---------------------------------------------------------------- FR-01

/// TC-01 — sessions are enumerated from the registry alone, with no user configuration.
#[test]
fn tc_01_enumerate_sessions_from_the_registry_alone() {
    let found = recorded();
    assert_eq!(
        found.session_count(),
        3,
        "the recording holds 3 live sessions of 17 entries; found {:?}",
        found.workspaces,
    );
    for (_, session) in found.sessions() {
        assert!(!session.session_id.is_empty(), "every session keeps its id");
        assert!(session.pid > 0, "every session keeps its pid");
    }
}

// ---------------------------------------------------------------- FR-02

/// TC-02 — non-VS-Code entrypoints produce no board entry at all.
///
/// FR-02's rule is stated here as a literal, from `docs/observation-sources.md` §2.1: a
/// session belongs on the board when its `entrypoint` is `claude-vscode` **and** its `kind`
/// is `interactive`. It is deliberately not expressed by calling
/// `RegistryEntry::is_editor_session`, which is the thing under test — an earlier version of
/// this case did exactly that and could not fail: with the predicate weakened, the set of
/// entries it expected to be excluded shrank to match, and the test agreed with the bug.
///
/// The recording carries one entry for each half of the rule — a `claude-cli` session that
/// *is* interactive, and a `claude-vscode` session that is *not* — so dropping either half
/// alone is visible here. Both are made live, so the filter is the only thing that can keep
/// them off the board.
#[test]
fn tc_02_non_vscode_entrypoints_are_excluded() {
    let all = entries();
    let every_pid: Vec<u32> = all.iter().map(|(_, e)| e.pid).collect();
    let found = discover(&just_entries(&all), &running(&all, &every_pid));

    let excluded: Vec<&RegistryEntry> = all
        .iter()
        .map(|(_, e)| e)
        .filter(|e| {
            e.entrypoint.as_deref() != Some("claude-vscode")
                || e.kind.as_deref() != Some("interactive")
        })
        .collect();

    assert!(
        excluded
            .iter()
            .any(|e| e.entrypoint.as_deref() != Some("claude-vscode")),
        "the recording is supposed to carry a non-VS-Code entrypoint",
    );
    assert!(
        excluded
            .iter()
            .any(|e| e.kind.as_deref() != Some("interactive")),
        "the recording is supposed to carry a non-interactive session",
    );

    for entry in &excluded {
        assert!(
            !found
                .sessions()
                .any(|(_, s)| s.session_id == entry.session_id),
            "{} (entrypoint {:?}, kind {:?}) reached the board",
            entry.session_id,
            entry.entrypoint,
            entry.kind,
        );
    }
    assert_eq!(
        found.filtered,
        excluded.len(),
        "every excluded entry is counted as filtered, and nothing else is",
    );
}

// ---------------------------------------------------------------- FR-03

/// TC-03 — an entry appearing or vanishing between passes changes the set, with no restart.
#[test]
fn tc_03_the_set_changes_between_passes() {
    let all = entries();
    let editor: Vec<u32> = all
        .iter()
        .filter(|(_, e)| e.is_editor_session())
        .map(|(_, e)| e.pid)
        .take(2)
        .collect();
    let entries = just_entries(&all);

    let first = discover(&entries, &running(&all, &editor[..1]));
    let then = discover(&entries, &running(&all, &editor));
    let after = discover(&entries, &running(&all, &editor[1..]));

    assert_eq!(first.session_count(), 1);
    assert_eq!(then.session_count(), 2, "an appearing process is picked up");
    assert_eq!(after.session_count(), 1, "a vanished one is dropped");
}

// ---------------------------------------------------------------- FR-04

/// TC-04 — the grouping key is the `cwd` and the label is its last segment.
#[test]
fn tc_04_group_label_is_the_last_segment() {
    let found = recorded();
    let sample = found
        .workspaces
        .iter()
        .find(|w| w.key == "C:/work/sample-repo")
        .expect("the recording holds a session in C:/work/sample-repo");

    assert_eq!(sample.label, "sample-repo");
    assert!(
        !sample.label.contains(['/', '\\']),
        "the label is a folder name, never a path",
    );
    assert!(
        !sample.label.contains("C-work"),
        "the label never comes from the lossy directory name under projects/",
    );
}

/// TC-04, supporting — the label rule itself, including the shapes a `cwd` can take.
#[test]
fn tc_04_workspace_label_cases() {
    assert_eq!(workspace_label("C:/work/sample-repo"), "sample-repo");
    assert_eq!(workspace_label("C:\\work\\sample-repo"), "sample-repo");
    assert_eq!(workspace_label("C:/work/sample-repo/"), "sample-repo");
    assert_eq!(workspace_label("/home/dev/my_project"), "my_project");
    // A bare root has no segment; it keeps its own text rather than becoming an empty header.
    assert_eq!(workspace_label("/"), "/");
}

// ---------------------------------------------------------------- FR-05 / FR-07

/// TC-05 — several sessions per group, and several groups, coexist.
#[test]
fn tc_05_several_sessions_per_group_and_several_groups() {
    let all = entries();
    let mut by_folder: Vec<u32> = all
        .iter()
        .filter(|(_, e)| e.is_editor_session() && e.cwd == "C:/work/sample-repo")
        .map(|(_, e)| e.pid)
        .take(2)
        .collect();
    assert_eq!(
        by_folder.len(),
        2,
        "the fixture has two entries in one folder"
    );
    let other = all
        .iter()
        .find(|(_, e)| e.is_editor_session() && e.cwd != "C:/work/sample-repo")
        .map(|(_, e)| e.pid)
        .expect("the fixture has an entry in another folder");
    by_folder.push(other);

    let found = discover(&just_entries(&all), &running(&all, &by_folder));

    assert_eq!(found.workspaces.len(), 2, "two folders, two groups");
    assert_eq!(found.session_count(), 3);
    let shared = found
        .workspaces
        .iter()
        .find(|w| w.key == "C:/work/sample-repo")
        .expect("the shared folder is a group");
    assert_eq!(
        shared.sessions.len(),
        2,
        "both sessions land in the one group"
    );
}

/// TC-07 — two editors on one folder are one group, not two.
#[test]
fn tc_07_two_editors_on_one_folder_are_one_group() {
    let all = entries();
    let pair: Vec<u32> = all
        .iter()
        .filter(|(_, e)| e.is_editor_session() && e.cwd == "C:/work/sample-repo")
        .map(|(_, e)| e.pid)
        .take(2)
        .collect();

    let found = discover(&just_entries(&all), &running(&all, &pair));
    assert_eq!(
        found.workspaces.len(),
        1,
        "identical cwd → exactly one group"
    );
    assert_eq!(found.workspaces[0].sessions.len(), 2);
}

// ---------------------------------------------------------------- FR-06 / FR-40

/// TC-06 — a session that is not running occupies no space.
#[test]
fn tc_06_a_session_that_is_not_running_occupies_no_space() {
    let all = entries();
    let found = discover(&just_entries(&all), &[]);

    assert_eq!(found.session_count(), 0);
    assert!(
        found.workspaces.is_empty(),
        "no group is created for a dead entry"
    );
}

/// TC-08 — stale entries are ignored, and are **not** reported as an error.
///
/// Two halves: 14 of the 17 recorded entries have dead processes, and a live PID whose
/// `procStart` disagrees is treated as dead, because PIDs are reused.
#[test]
fn tc_08_stale_entries_are_ignored() {
    let all = entries();
    let found = recorded();

    let editor_entries = all.iter().filter(|(_, e)| e.is_editor_session()).count();
    assert_eq!(
        found.stale + found.session_count(),
        editor_entries,
        "every editor entry is either drawn or counted stale, and none is lost",
    );
    assert!(found.stale > 0, "the recording is mostly stale entries");

    // PID reuse: the process is running, but it is not the one the entry described.
    let (_, live_entry) = all
        .iter()
        .find(|(live, e)| *live && e.is_editor_session())
        .expect("the recording holds a live editor session");
    let impostor = vec![LiveProcess {
        pid: live_entry.pid,
        // The same PID, started at a different time: a different process wearing the number.
        started_unix_seconds: live_entry.proc_start_unix_seconds().map(|s| s + 3600),
    }];
    let reused = discover(&just_entries(&all), &impostor);
    assert_eq!(
        reused.session_count(),
        0,
        "a live PID whose procStart disagrees must be treated as dead",
    );
}
