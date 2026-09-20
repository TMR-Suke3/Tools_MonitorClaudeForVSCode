//! The process probe (FR-40, and the PID-reuse guard behind TC-08).
//!
//! There is no fixture for this one and there cannot be: a recording of a process table is
//! a recording of a machine that no longer exists. So these cases use the **only process
//! whose identity the test knows for certain — itself** — and check the properties the guard
//! depends on rather than any particular value.
//!
//! TC-08 already covers what happens to a session whose process does not match, from the
//! recorded registry. What is left to check here is that the probe reports the truth.

use mcv_board::watch::process::ProcessTable;
use mcv_core::session::{LiveProcess, RegistryEntry};

/// A PID that is running is reported, with a start time that is not obviously nonsense.
#[test]
fn the_running_process_is_found_with_a_start_time() {
    let me = std::process::id();
    let mut table = ProcessTable::new();

    let live = table.live_among(&[me]);
    let found = live
        .iter()
        .find(|p| p.pid == me)
        .expect("this test's own process is running");

    let started = found
        .started_unix_seconds
        .expect("a process table that cannot say when a process started cannot guard PID reuse");
    // 2020-01-01 .. 2100-01-01. Wide on purpose: the point is that the value is a Unix
    // timestamp in seconds, not milliseconds and not something since boot.
    assert!(
        (1_577_836_800..4_102_444_800).contains(&started),
        "start time {started} is not a plausible Unix timestamp in seconds",
    );
}

/// A PID that is not running is simply absent — not an error, and not a guess.
#[test]
fn an_unused_pid_is_absent() {
    let mut table = ProcessTable::new();

    // Above every PID Windows and Linux hand out; if this ever does name a process, the
    // assertion below is the thing that should fail rather than something subtler later.
    let impossible = u32::MAX - 1;
    let live = table.live_among(&[impossible]);

    assert!(
        !live.iter().any(|p| p.pid == impossible),
        "a PID nothing is using must not be reported as running",
    );
}

/// Asking about nothing costs nothing, and does not report anything.
#[test]
fn an_empty_query_returns_nothing() {
    let mut table = ProcessTable::new();
    assert_eq!(table.live_among(&[]), Vec::new());
}

/// The probe and the core agree: what the table reports about a running process satisfies
/// the core's identity check for a registry entry describing that same process.
///
/// This is the join the whole guard rests on, and it is the piece neither side can test
/// alone — the core has no process table and the probe has no registry.
#[test]
fn the_probe_and_the_core_agree_about_identity() {
    let me = std::process::id();
    let mut table = ProcessTable::new();
    let live = table.live_among(&[me]);
    let found = live.iter().find(|p| p.pid == me).expect("this process");
    let started = found.started_unix_seconds.expect("a start time");

    // The same instant, written the way the registry writes it: a Windows FILETIME.
    let filetime = (started + 11_644_473_600) * 10_000_000;
    let entry = RegistryEntry::parse(&format!(
        r#"{{"pid":{me},"sessionId":"s","cwd":"C:/work/sample-repo",
            "procStart":"{filetime}","kind":"interactive","entrypoint":"claude-vscode"}}"#
    ))
    .expect("the entry parses");

    assert!(
        found.matches(&entry),
        "the probe's start time and the registry's FILETIME describe the same process",
    );

    let impostor = LiveProcess {
        pid: me,
        started_unix_seconds: Some(started + 3600),
    };
    assert!(
        !impostor.matches(&entry),
        "the same PID started an hour apart is a different process",
    );
}
