//! TC-95 … TC-97 — what the board makes of the helper's event log.
//!
//! The reading itself belongs to `Tail` and is tested in `tail.rs`; this is the half with the
//! judgement in it — which session a line is about, which lines mean nothing, and the one
//! case a byte offset cannot cover.

use mcv_board::watch::live::HookLog;
use mcv_board::watch::tail::Polled;
use mcv_core::event::HookSignal;

/// A poll of the log, as the tail would have delivered it.
fn polled(lines: &[&str], restarted: bool) -> Polled {
    Polled {
        lines: lines
            .iter()
            .enumerate()
            .map(|(i, line)| (i as u64, (*line).to_owned()))
            .collect(),
        bytes_read: 0,
        undecodable: Vec::new(),
        restarted,
    }
}

/// TC-95 (FR-36) — lines are sorted by the session they are about.
///
/// The log is shared by every Claude Code session on the machine, terminal ones included.
/// The board keeps what belongs to the sessions it shows and is not confused by the rest.
#[test]
fn tc_95_events_are_grouped_by_session() {
    let mut log = HookLog::default();
    let by_session = log.absorb(&polled(
        &[
            r#"{"at":10,"event":"UserPromptSubmit","session":"a"}"#,
            r#"{"at":11,"event":"Notification","session":"b","kind":"permission_prompt"}"#,
            r#"{"at":12,"event":"Stop","session":"a"}"#,
        ],
        false,
    ));

    assert_eq!(
        by_session.get("a").map(Vec::as_slice),
        Some([HookSignal::PromptSubmitted, HookSignal::TurnStopped].as_slice()),
        "in the order they were written, which is the order they happened",
    );
    assert_eq!(
        by_session.get("b").map(Vec::as_slice),
        Some([HookSignal::NeedsUser].as_slice()),
    );
    assert_eq!(log.last_at, Some(12), "and the hooks are alive (FR-52)");
}

/// TC-96 (FR-52) — a line this build cannot use still proves the hooks are alive.
///
/// FR-52 asks whether events are *arriving*, not whether this version understood them. A
/// board that only counted the lines it could act on would announce that the hooks had gone
/// quiet on a machine whose sessions were doing nothing but notify.
#[test]
fn tc_96_an_unusable_line_is_still_a_sign_of_life() {
    let mut log = HookLog::default();
    let by_session = log.absorb(&polled(
        &[
            r#"{"at":20,"event":"Notification","session":"a","kind":"idle"}"#,
            r#"{"at":21,"event":"SomethingNewer","session":"a"}"#,
            "half a line, as a file three processes append to look",
        ],
        false,
    ));

    assert!(
        by_session.is_empty(),
        "none of those means anything in the state model",
    );
    assert_eq!(
        log.last_at,
        Some(21),
        "but two of them are events that arrived",
    );
}

/// TC-97 (FR-52) — a trimmed log does not replay itself.
///
/// Trimming rewrites the file, which puts the tail's offset back to zero and re-delivers
/// everything still in it. Folding those in again would report a prompt that was answered
/// long ago — the one failure this file must not have, because the log records that Claude
/// asked and nothing in it records that the user replied.
#[test]
fn tc_97_a_replaced_log_does_not_summon_anybody_twice() {
    let prompt = r#"{"at":30,"event":"Notification","session":"a","kind":"permission_prompt"}"#;
    let mut log = HookLog::default();

    let first = log.absorb(&polled(&[prompt], false));
    assert_eq!(
        first.get("a").map(Vec::as_slice),
        Some([HookSignal::NeedsUser].as_slice())
    );

    // The same line, plus one written after the trim.
    let again = log.absorb(&polled(
        &[prompt, r#"{"at":40,"event":"Stop","session":"a"}"#],
        true,
    ));
    assert_eq!(
        again.get("a").map(Vec::as_slice),
        Some([HookSignal::TurnStopped].as_slice()),
        "the prompt is old news; only what was written after it is new",
    );

    // ...and an ordinary poll is not deduplicated: two prompts in a row are two prompts.
    let ordinary = log.absorb(&polled(&[prompt], false));
    assert_eq!(
        ordinary.get("a").map(Vec::as_slice),
        Some([HookSignal::NeedsUser].as_slice()),
        "outside a restart the byte offset is the guarantee, and it does not repeat itself",
    );
}

/// TC-97b (FR-52) — a trimmed log does not drop a line that shares a millisecond.
///
/// The dedupe after a restart cannot key on the timestamp alone. The helper stamps in whole
/// milliseconds and several processes append to one file, so two events sharing a stamp is
/// ordinary — and one of them can be the new one. Dropping everything at the boundary
/// millisecond loses a real event; keeping everything re-delivers an answered prompt.
#[test]
fn tc_97b_a_line_sharing_the_boundary_millisecond_is_not_lost() {
    let seen = r#"{"at":50,"event":"Stop","session":"a"}"#;
    let also_at_50 = r#"{"at":50,"event":"UserPromptSubmit","session":"b"}"#;

    let mut log = HookLog::default();
    log.absorb(&polled(&[seen], false));

    // The log is rewritten. It hands back the line already read, plus one written in the same
    // millisecond by another session that this board had not seen yet.
    let after = log.absorb(&polled(&[seen, also_at_50], true));

    assert_eq!(
        after.get("a"),
        None,
        "the line already folded in must not be delivered twice",
    );
    assert_eq!(
        after.get("b").map(Vec::as_slice),
        Some([HookSignal::PromptSubmitted].as_slice()),
        "...but a different line at the same stamp is a different event, and is not old news",
    );
}

/// TC-97c (FR-36) — a signal for a session the board has not discovered yet is offered again.
///
/// A session registers a moment before discovery sees it, and hook events do not wait: a
/// `SessionStart` fires at once, and a permission prompt in the first second of a session is
/// not hypothetical. The log is tailed by byte offset, so a signal dropped because nobody
/// claimed it is gone for good.
#[test]
fn tc_97c_a_signal_for_an_undiscovered_session_survives_one_poll() {
    let line = r#"{"at":10,"event":"Notification","session":"new","kind":"permission_prompt"}"#;

    let mut log = HookLog::default();
    let first = log.absorb(&polled(&[line], false));
    assert_eq!(
        first.get("new").map(Vec::as_slice),
        Some([HookSignal::NeedsUser].as_slice()),
    );

    // The board had no such session, so it hands the whole lot back.
    log.keep_unclaimed(first);

    // Next poll: nothing new in the file, and the signal is offered again.
    let fresh = log.absorb(&polled(&[], false));
    let offered = log.offer(fresh);
    assert_eq!(
        offered.get("new").map(Vec::as_slice),
        Some([HookSignal::NeedsUser].as_slice()),
        "the session has been discovered by now, and the prompt is still its news",
    );

    // ...and only once. The log is shared with terminal sessions this board never shows, and
    // their lines arrive forever: carrying them indefinitely would be a leak with a slow fuse.
    let empty = log.absorb(&polled(&[], false));
    let nothing = log.offer(empty);
    assert!(
        nothing.is_empty(),
        "an unclaimed signal is offered a second time, not for ever",
    );
}
