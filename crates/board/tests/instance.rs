//! TC-128, TC-129 — one board per machine (FR-60).
//!
//! This is one of the few things in the product that cannot be checked by a function: whether
//! a *second process* stops itself is a fact about two processes. So the real binary is
//! launched twice, and what the operating system does with them is what the case asserts.
//!
//! Measured on the author's machine before the guard existed: two boards, both restored to the
//! same remembered position and drawn on top of each other, both writing the same settings
//! file, and the second unable to register either shortcut because the first already held
//! them — which the menu then reported as "not registered" while the keys did in fact work,
//! for the other board. Nothing on screen said there were two.
//!
//! **The case skips rather than fails when it cannot run**, and says so, exactly as TC-101
//! does for a machine with no desktop. There are two ways for it to be unrunnable, and both
//! are ordinary: a headless agent has no window station, and a developer running the tests
//! with their own board open is a developer whose board already holds the guard the first
//! instance here would need.
#![cfg(windows)]

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use mcv_board::win::{owner_of, top_level_windows, visible_windows};

/// The board's window title, from `tauri.conf.json`.
const TITLE: &str = "Claude session board";

/// Long enough for a debug build to open a window on a busy machine, and short enough that a
/// hung case is a failing case rather than a hanging suite.
const PATIENCE: Duration = Duration::from_secs(30);

/// How many board windows belong to one process.
///
/// **By process, not by title.** The developer running this may well have a board of their own
/// open, and it has the same title; counting titles alone would make this case pass or fail
/// on something that has nothing to do with it.
fn board_windows_of(pid: u32) -> usize {
    visible_windows()
        .into_iter()
        .filter(|(window, title)| title == TITLE && owner_of(*window) == pid)
        .count()
}

fn wait_for(mut what: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// A launched board that is killed when it goes out of scope, however it goes out of scope.
///
/// The assertions below are deliberately made after both children have been taken down, so the
/// ordinary path does not need this. It is here for the paths that are not ordinary: a panic
/// between the two launches, or in one of the Win32 helpers, would otherwise strand a board on
/// the developer's desk — and a stranded board holds the very guard the next run needs.
struct Launched(Child);

impl Drop for Launched {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The recordings rather than the live sessions: this case is about processes, and a board
/// reading the machine's real transcripts to answer it would be doing work nobody asked for.
fn launch() -> Launched {
    Launched(
        Command::new(env!("CARGO_BIN_EXE_mcv-board"))
            .arg("--demo")
            .spawn()
            .expect("the board binary is built by the test harness"),
    )
}

#[test]
fn a_second_launch_does_not_start_a_second_board() {
    // **Asked before anything is launched.** A probe started while the developer's own board is
    // running does not fail quietly: it becomes a single-instance client, and the first thing
    // it does is summon that board — moving it to a corner and flashing it. Running the tests
    // is not a thing that should move someone's windows, so the skip is decided by looking
    // rather than by launching and seeing what happens.
    //
    // Hidden windows count. A board that is running and out of sight is still the board, and
    // still holds the guard this case needs to take for itself.
    if let Some((_, title)) = top_level_windows().iter().find(|(_, t)| t == TITLE) {
        println!(
            "skipped: a board is already running on this machine ({title:?}) and holds the guard"
        );
        return;
    }

    let mut first = launch();
    let first_pid = first.0.id();
    if !wait_for(|| board_windows_of(first_pid) == 1) {
        println!("skipped: the first board never opened a window — no desktop to open one on");
        return;
    }

    let mut second = launch();
    let second_pid = second.0.id();
    // Watched **while** it is running, not once it has gone: a second board that appeared and
    // then withdrew would leave nothing behind to find afterwards.
    //
    // Sampling cannot prove a window never existed — one opened and destroyed between two polls
    // is one this never sees. What it is asked to catch is the failure the guard exists for,
    // and that failure is a board that *stays*: a Tauri window, once opened, is there until its
    // process ends. A flicker shorter than 100 ms is not a shape this code can produce.
    let mut ever_drew = 0;
    let stopped = wait_for(|| {
        ever_drew = ever_drew.max(board_windows_of(second_pid));
        matches!(second.0.try_wait(), Ok(Some(_)))
    });
    // **How** it stopped. Any crash on startup — a missing runtime, a bad configuration, a
    // panic — would also stop it, draw nothing, and leave the first board alone, and this case
    // would then pass while proving that the machine cannot run two boards for a reason that
    // has nothing to do with the guard.
    let exit = second.0.try_wait().ok().flatten();

    // Read before anything is torn down: the assertions below are about the state while both
    // have been launched.
    let first_alive = matches!(first.0.try_wait(), Ok(None));
    let first_windows = board_windows_of(first_pid);
    drop(second);
    drop(first);

    // TC-128 — the second process stops itself, cleanly, and the first is untouched by it.
    assert!(
        stopped,
        "the second launch must exit on its own rather than become a second board"
    );
    assert!(
        exit.is_some_and(|status| status.success()),
        "the second launch must stand down, not fall over: {exit:?}"
    );
    assert!(
        first_alive,
        "the board that was already running must not be taken down by a second launch"
    );

    // TC-129 — and there is one board on screen, not two, at every moment this looked.
    assert_eq!(first_windows, 1, "the first board is still the board");
    assert_eq!(ever_drew, 0, "the second launch drew no window that lasted");
}
