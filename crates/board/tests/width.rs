//! Two widths, and when the board is entitled to the narrow one (§2.5, ADR-0015).
//!
//! `docs/ui-overlay.md` §2.5 asks for **two discrete widths and no more**: 300 while any
//! group is open, and the longest workspace name plus its chrome while everything is folded,
//! clamped to [140, 300]. Two discrete widths rather than a continuous fit, because a board
//! that re-measured itself every time a title changed would twitch in the corner of the eye
//! all day — and titles change mid-session.
//!
//! **Two widths in a second sense, since §2.7.** The window is larger than the panel — there
//! is a transparent margin around it for the wave that leaves an `awaiting_user` indicator.
//! The clamp is a statement about the *panel*, so that is what these cases assert; that the
//! window is the panel plus its margins is asserted once, on its own, rather than mixed into
//! every figure.

use mcv_board::view::{GroupState, Inspect, MARGIN, MIN_WIDTH, OPEN_WIDTH, SessionState, build};
use mcv_core::palette::{Status, Theme};

fn session(id: &'static str) -> SessionState<'static> {
    SessionState {
        id,
        status: Status::Working,
        ai_title: Some("a title"),
        registry_name: None,
        prompt_line: None,
        derived_name: None,
        time: "12s".to_owned(),
    }
}

fn board(groups: Vec<GroupState<'static>>) -> mcv_board::view::BoardView {
    build(&groups, Theme::Dark, Inspect::default())
}

#[test]
fn a_board_with_anything_open_is_the_open_width() {
    let view = board(vec![GroupState {
        key: "C:/work/sample-repo",
        label: "sample-repo",
        folded: false,
        sessions: vec![session("a"), session("b")],
    }]);

    assert!(!view.fully_folded);
    assert_eq!(view.panel_width(), OPEN_WIDTH);
}

/// The window is the panel plus the transparent margin on either side (§2.7).
///
/// Asserted here and nowhere else. Every other case in this file is about how wide the *board*
/// is, and mixing the margin into those numbers would make each of them a test of two things.
#[test]
fn the_window_is_the_panel_plus_its_margins() {
    let view = board(vec![GroupState {
        key: "C:/work/sample-repo",
        label: "sample-repo",
        folded: false,
        sessions: vec![session("a"), session("b")],
    }]);

    assert_eq!(view.width(), view.panel_width() + MARGIN * 2);
    assert_eq!(view.height(), view.panel_height() + MARGIN * 2);
    // The margin is what the wave is drawn in; at zero the board would be clipped at its own
    // edge again, which is the thing §2.7 exists to prevent.
    const { assert!(MARGIN > 0) };
}

/// A one-session workspace has no fold state at all (§2.3), so a board made only of merged
/// rows has not been folded by anyone — the titles stay and the width stays.
#[test]
fn a_board_of_merged_rows_alone_is_not_a_folded_board() {
    let view = board(vec![
        GroupState {
            key: "C:/work/one",
            label: "one",
            folded: false,
            sessions: vec![session("a")],
        },
        GroupState {
            key: "C:/work/two",
            label: "two",
            folded: false,
            sessions: vec![session("b")],
        },
    ]);

    assert!(
        !view.fully_folded,
        "nothing here can fold, so nothing has been folded",
    );
    assert_eq!(view.panel_width(), OPEN_WIDTH);
}

#[test]
fn folding_every_foldable_group_makes_the_board_folded() {
    let mut view = board(vec![
        GroupState {
            key: "C:/work/sample-repo",
            label: "sample-repo",
            folded: false,
            sessions: vec![session("a"), session("b")],
        },
        GroupState {
            key: "C:/work/other-repo",
            label: "other-repo",
            folded: false,
            sessions: vec![session("c"), session("d")],
        },
        GroupState {
            key: "C:/work/merged",
            label: "merged",
            folded: false,
            sessions: vec![session("e")],
        },
    ]);

    view.set_folded("C:/work/sample-repo", true);
    assert!(
        !view.fully_folded,
        "one of two foldable groups is still open"
    );

    view.set_folded("C:/work/other-repo", true);
    assert!(
        view.fully_folded,
        "both foldable groups are folded; the merged row does not hold it open",
    );

    view.set_folded("C:/work/other-repo", false);
    assert!(!view.fully_folded, "unfolding one opens the board again");
}

/// A merged row has no fold state, so asking it to fold does nothing at all (§2.3).
#[test]
fn a_merged_row_cannot_be_folded() {
    let mut view = board(vec![GroupState {
        key: "C:/work/merged",
        label: "merged",
        folded: false,
        sessions: vec![session("a")],
    }]);

    view.set_folded("C:/work/merged", true);
    assert!(!view.groups[0].folded);
    assert!(!view.fully_folded);
}

/// The measured width is used only while the board is folded, and only within the clamp.
#[test]
fn the_narrow_width_is_the_measurement_inside_the_clamp() {
    let mut view = board(vec![GroupState {
        key: "C:/work/sample-repo",
        label: "sample-repo",
        folded: true,
        sessions: vec![session("a"), session("b")],
    }]);
    assert!(view.fully_folded);

    view.folded_width = Some(186);
    assert_eq!(view.panel_width(), 186);

    view.folded_width = Some(40);
    assert_eq!(
        view.panel_width(),
        MIN_WIDTH,
        "a short name does not make a sliver"
    );

    view.folded_width = Some(1000);
    assert_eq!(
        view.panel_width(),
        OPEN_WIDTH,
        "and it never grows past the open width"
    );

    view.folded_width = None;
    assert_eq!(
        view.panel_width(),
        OPEN_WIDTH,
        "before the renderer has measured anything, the board stays where it was",
    );
}

/// The measurement belongs to a set of workspaces, and is recomputed only when that set
/// changes — not when a status or a title moves (ADR-0015).
#[test]
fn the_signature_follows_the_workspaces_and_nothing_else() {
    let mut view = board(vec![
        GroupState {
            key: "C:/work/sample-repo",
            label: "sample-repo",
            folded: true,
            sessions: vec![session("a"), session("b")],
        },
        GroupState {
            key: "C:/work/other-repo",
            label: "other-repo",
            folded: true,
            sessions: vec![session("c"), session("d")],
        },
    ]);
    let before = view.workspace_signature();

    // A status changes, and a title with it. The workspaces have not changed.
    let mut moved = board(vec![
        GroupState {
            key: "C:/work/sample-repo",
            label: "sample-repo",
            folded: true,
            sessions: vec![SessionState {
                status: Status::AwaitingUser,
                ai_title: Some("something else entirely"),
                ..session("a")
            }],
        },
        GroupState {
            key: "C:/work/other-repo",
            label: "other-repo",
            folded: true,
            sessions: vec![session("c"), session("d")],
        },
    ]);
    assert_eq!(
        moved.workspace_signature(),
        before,
        "a board that re-measured itself on a status change would twitch all day",
    );

    // A workspace appears. That is the one thing that must invalidate the measurement.
    moved.groups.push(view.groups.remove(0));
    assert_ne!(moved.workspace_signature(), before);
}

/// The board is drawn at the size the user chose (§2.8).
///
/// One multiplication, at the end: `panel_width` and `panel_height` are the design's own
/// numbers, and the scale is applied to the total rather than to each part, so the rounding
/// happens once instead of accumulating down a full board.
///
/// **And that one rounding goes to the nearest pixel.** The sizes below are the four §2.8
/// publishes for a three-row board, written out as literals — this used to assert
/// `hundred * 150 / 100`, which reproduced the code's own integer division and so agreed with
/// it about 220.5 px whichever way that went. A test that computes the expectation the way
/// the code does cannot catch the code being wrong.
#[test]
fn the_window_follows_the_chosen_size() {
    let mut view = board(vec![GroupState {
        key: "C:/work/sample-repo",
        label: "sample-repo",
        folded: false,
        sessions: vec![session("a"), session("b")],
    }]);

    assert_eq!(view.width(), view.panel_width() + MARGIN * 2);
    assert_eq!(
        view.panel_width(),
        OPEN_WIDTH,
        "the panel's own figure is the design's, and the scale is applied on top of it",
    );

    // §2.8: 100 % 332 x 147, 125 % 415 x 184, 150 % 498 x 221, 200 % 664 x 294. The 125 %
    // and 150 % heights are the cases that matter — 183.75 and 220.5 — and flooring either
    // leaves the window shorter than the rows it holds.
    for (percent, width, height) in [
        (100, 332, 147),
        (125, 415, 184),
        (150, 498, 221),
        (200, 664, 294),
    ] {
        view.scale = percent;
        assert_eq!(
            (view.width(), view.height()),
            (width, height),
            "the board at {percent} %",
        );
    }
}
