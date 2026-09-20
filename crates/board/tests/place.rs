//! Where the board goes, and when it is brought back (FR-28, FR-32, TC-113 … TC-127).
//!
//! The bug behind every case here is one the user reported as *the window is not there any
//! more*, which has three explanations that look identical from the outside: it was hidden,
//! the display it was on went away, or it is exactly where it should be and something else is
//! wrong. None of the three can be reproduced by unplugging a monitor inside a test, so the
//! arithmetic is separated from the platform and it is the arithmetic that is checked here —
//! the same split that lets TC-68 test the restart case without a second screen.
//!
//! The cases that matter most are the **negative** ones. A rescue that fires when it should
//! not is a board that walks away from where its owner put it, which is a worse product than
//! one that occasionally has to be summoned back.

use mcv_board::place::{
    Click, GRAB, INSET, Whereabouts, contain_growth, reachable, rescue, summon_position,
    tray_click, where_line, whereabouts,
};
use mcv_board::settings::Area;

/// A single 1920 × 1080 display with a 48 px taskbar along the bottom.
fn one_screen() -> Vec<Area> {
    vec![Area {
        x: 0,
        y: 0,
        width: 1920,
        height: 1032,
    }]
}

/// The author's desk: a 2560 × 1440 display with a second one to its *left*, which is where
/// the negative coordinates that broke this arithmetic once already come from.
fn two_screens() -> Vec<Area> {
    vec![
        Area {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1032,
        },
        Area {
            x: 0,
            y: 0,
            width: 2560,
            height: 1392,
        },
    ]
}

const BOARD: (u32, u32) = (300, 310);

// TC-123 — a summoned board lands in the work area's top-right corner.
#[test]
fn a_summoned_board_lands_in_the_top_right_corner() {
    let work = one_screen()[0];
    let (x, y) = summon_position(work, BOARD);

    assert_eq!(x, 1920 - 300 - INSET, "inset from the right edge");
    assert_eq!(y, INSET, "inset from the top edge");
    assert!(
        reachable((x, y), BOARD, &one_screen()),
        "and it is somewhere the mouse can reach"
    );
}

// TC-117 — the work area is what is used, not the display's bounds: a board summoned to the
// bottom-right would otherwise sit under the taskbar.
#[test]
fn the_taskbar_is_not_part_of_the_work_area() {
    let work = Area {
        x: 0,
        y: 48,
        width: 1920,
        height: 984,
    };
    let (_, y) = summon_position(work, BOARD);
    assert_eq!(y, 48 + INSET, "measured from the work area's own top");
}

// TC-124 — a display left of the origin has negative coordinates, and the corner is still its
// own corner rather than the virtual desktop's.
#[test]
fn a_display_left_of_the_origin_still_gets_its_own_corner() {
    let left = two_screens()[0];
    let (x, y) = summon_position(left, BOARD);

    assert_eq!(x, -1920 + 1920 - 300 - INSET);
    assert_eq!(y, INSET);
    assert!(x < 0, "the whole display is left of the origin");
    assert!(reachable((x, y), BOARD, &two_screens()));
}

// TC-125 — a board larger than the display it is summoned to is put at the corner, not pushed
// off the far edge by the inset.
#[test]
fn a_board_larger_than_the_display_is_still_on_it() {
    let small = Area {
        x: 100,
        y: 100,
        width: 200,
        height: 200,
    };
    let (x, y) = summon_position(small, BOARD);

    assert_eq!((x, y), (100, 100), "the corner itself, inset abandoned");
    assert!(reachable((x, y), BOARD, &[small]));
}

// TC-113 — a board on a live display is not touched. The rescue runs every few seconds, so
// this is the case it spends almost all of its time in.
#[test]
fn a_board_on_a_live_display_is_left_alone() {
    assert_eq!(rescue((1500, 40), BOARD, &one_screen()), None);
    assert_eq!(rescue((-1800, 200), BOARD, &two_screens()), None);
}

// TC-114 — the display the board was on has gone away.
#[test]
fn a_board_on_a_vanished_display_comes_back() {
    // Where it sat while the second screen was plugged in.
    let was = (-1600, 300);
    assert!(!reachable(was, BOARD, &one_screen()));

    let Some((x, y)) = rescue(was, BOARD, &one_screen()) else {
        panic!("a board on no display must be brought back");
    };
    assert!(reachable((x, y), BOARD, &one_screen()));
    // Nearest, not primary-corner: it comes back at the edge it disappeared over.
    assert_eq!(x, 0, "pushed just inside the display it was nearest to");
    assert_eq!(y, 300, "and no further up than it had to move");

    // Fully inside, not merely past the threshold. `GRAB` decides *whether* to move the
    // board; it does not decide where it lands. A rescue that stopped the moment 48 px of
    // the board showed would leave it 250 px off the screen — reachable, and useless — and
    // the failure being repaired is *I cannot see the board*.
    let screen = &one_screen()[0];
    assert!(
        x >= screen.x && x + i32::try_from(BOARD.0).unwrap() <= screen.x + screen.width as i32,
        "the rescued board is wholly on the display, not clinging to its edge"
    );
}

// TC-115 — a board straddling two displays is where its owner put it.
#[test]
fn a_board_straddling_two_displays_is_not_rescued() {
    // Half on each screen of `two_screens`, over the seam at x = 0.
    let straddling = (-150, 400);
    assert!(reachable(straddling, BOARD, &two_screens()));
    assert_eq!(rescue(straddling, BOARD, &two_screens()), None);
}

// TC-116 — a board deliberately parked half off an edge is also not rescued. Someone keeping
// it out of the way has not lost it.
#[test]
fn a_board_parked_over_an_edge_is_not_rescued() {
    let over_the_bottom = (1500, 1032 - 60);
    assert!(reachable(over_the_bottom, BOARD, &one_screen()));
    assert_eq!(rescue(over_the_bottom, BOARD, &one_screen()), None);

    let over_the_right = (1920 - 60, 100);
    assert!(reachable(over_the_right, BOARD, &one_screen()));
    assert_eq!(rescue(over_the_right, BOARD, &one_screen()), None);
}

// TC-116b — the line between the two: less than a grab's worth showing is not something anyone
// can take hold of, and the board is brought back.
#[test]
fn less_than_a_grab_showing_is_not_reachable() {
    let barely = (1920 - GRAB.0, 100);
    assert!(reachable(barely, BOARD, &one_screen()), "exactly a grab");

    let not_quite = (1920 - GRAB.0 + 1, 100);
    assert!(!reachable(not_quite, BOARD, &one_screen()));
    assert!(rescue(not_quite, BOARD, &one_screen()).is_some());
}

// TC-117, second half — with no displays at all, every monitor asleep, there is nowhere to put
// the board, and it is left exactly as it is rather than moved to a coordinate invented out of
// nothing.
#[test]
fn with_no_displays_the_board_is_left_where_it_is() {
    assert_eq!(rescue((100, 100), BOARD, &[]), None);
    assert!(!reachable((100, 100), BOARD, &[]));
}

// TC-126 — what a click on the tray icon does, in all three states.
#[test]
fn the_tray_click_summons_unless_the_board_is_already_in_front_of_you() {
    assert_eq!(tray_click(false, false), Click::Summon, "hidden");
    assert_eq!(
        tray_click(true, false),
        Click::Summon,
        "visible, but on no display — the case that used to be a silent no-op"
    );
    assert_eq!(tray_click(true, true), Click::Hide, "visible and reachable");
    // Hidden is hidden, whatever its remembered position says.
    assert_eq!(tray_click(false, true), Click::Summon);
}

// TC-127 — the board says which of the three states it is in, because they cannot be told
// apart from the outside and each has a different repair.
#[test]
fn the_board_says_where_it_is() {
    let screens = two_screens();

    assert_eq!(
        whereabouts(false, (100, 100), BOARD, &screens),
        Whereabouts::Hidden
    );
    assert_eq!(
        whereabouts(true, (100, 100), BOARD, &screens),
        Whereabouts::On { display: 1 },
        "the second entry is the display it is on"
    );
    assert_eq!(
        whereabouts(true, (-1800, 100), BOARD, &screens),
        Whereabouts::On { display: 0 }
    );
    assert_eq!(
        whereabouts(true, (5000, 100), BOARD, &screens),
        Whereabouts::Lost
    );

    // The display showing more of it is the one named.
    assert_eq!(
        whereabouts(true, (-250, 400), BOARD, &screens),
        Whereabouts::On { display: 0 },
        "250 of its 300 px are on the left-hand screen"
    );

    // Displays are numbered from one where a person can read them.
    assert_eq!(
        where_line(Whereabouts::On { display: 1 }),
        "The board is on display 2"
    );
    assert_eq!(where_line(Whereabouts::Hidden), "The board is hidden");
    assert_eq!(
        where_line(Whereabouts::Lost),
        "The board is off every display"
    );
}

// TC-127b — the line and the rescue are measured against the same edge, and the cases that
// have no obvious answer still have a fixed one.
//
// `whereabouts` repeats `reachable`'s threshold rather than sharing its code, so the two can
// drift apart: a board the rescue leaves alone while the menu calls it lost would be the
// product contradicting itself in the one place built to explain a board nobody can find.
#[test]
fn the_line_is_measured_against_the_same_edge_as_the_rescue() {
    let screens = one_screen();

    // Exactly a grab's worth over the top-left corner — 48 px of it across, 24 down.
    let corner = (-252, -286);
    assert_eq!(
        whereabouts(true, corner, BOARD, &screens),
        Whereabouts::On { display: 0 }
    );
    assert!(
        reachable(corner, BOARD, &screens),
        "the two answers are taken at the same edge"
    );

    // One pixel less, either way, and both say so.
    for short in [(corner.0 - 1, corner.1), (corner.0, corner.1 - 1)] {
        assert_eq!(whereabouts(true, short, BOARD, &screens), Whereabouts::Lost);
        assert!(!reachable(short, BOARD, &screens));
    }

    // Every monitor asleep. There is no display to name, and the board is not hidden — it is
    // exactly the "on screen somewhere nobody can look" case the line exists for.
    assert_eq!(whereabouts(true, (100, 100), BOARD, &[]), Whereabouts::Lost);

    // A display above the origin. Negative coordinates are not only a left-hand screen.
    let above = vec![Area {
        x: 0,
        y: -1080,
        width: 1920,
        height: 1032,
    }];
    assert_eq!(
        whereabouts(true, (100, -1000), BOARD, &above),
        Whereabouts::On { display: 0 }
    );

    // Split down the middle of a seam, with neither display showing more of it. Which one is
    // named does not matter; that it is always the same one does, because a line that flipped
    // between two displays while the board sat still would read as the board moving.
    let seam = vec![
        Area {
            x: 0,
            y: 0,
            width: 1000,
            height: 1000,
        },
        Area {
            x: 1000,
            y: 0,
            width: 1000,
            height: 1000,
        },
    ];
    assert_eq!(
        whereabouts(true, (850, 100), BOARD, &seam),
        Whereabouts::On { display: 1 },
        "a tie is broken by the later display, every time"
    );
}

// TC-119 — growing never walks the board off the screen, and never tidies away a board its
// owner put over an edge.
//
// Both reported from a desk with a 100 % display beside a 175 % one: a board summoned to the
// top-right corner came up through the right-hand edge, because the corner had been worked out
// from the size the board had on the display it left.
#[test]
fn a_resize_puts_back_only_what_the_resize_pushed_out() {
    let screens = one_screen();
    let work = &screens[0];

    // Summoned to the top-right with INSET to spare, then 75 % wider — the scale factor of the
    // display it arrived on — and 233 px taller.
    let corner = summon_position(*work, BOARD);
    let grown = (525, 543);
    let Some((x, y)) = contain_growth(corner, BOARD, grown, &screens) else {
        panic!("a board that grew through the edge must be brought back");
    };
    assert_eq!(
        x,
        work.x + work.width as i32 - grown.0 as i32,
        "back by exactly the overhang, flush with the edge it went through"
    );
    assert_eq!(y, corner.1, "and not moved on the axis that still fits");

    // The same growth away from the edge costs nothing.
    assert_eq!(contain_growth((200, 200), BOARD, grown, &screens), None);

    // A board parked half off the bottom was not put there by a resize. It is not on a display
    // whole to begin with, so nothing here applies to it — this is TC-116's restraint, kept
    // through a resize as well as through a rescue.
    let parked = (1500, 1032 - 60);
    assert_eq!(contain_growth(parked, BOARD, grown, &screens), None);

    // A board grown larger than the display it is on goes to that display's corner rather than
    // off the opposite edge: being on screen beats keeping the inset.
    let huge = (2000, 2000);
    assert_eq!(
        contain_growth(corner, BOARD, huge, &screens),
        Some((work.x, work.y))
    );
}

// TC-119b — the containment asks about *some* display, not the first one that happened to
// match, and it works on a display left of the origin like any other.
#[test]
fn growth_is_contained_against_whichever_display_still_holds_the_board() {
    // A virtual display sitting over part of a real one: work areas that overlap are how the
    // platform reports a second monitor mirrored or extended onto a headset.
    let overlapping = vec![
        Area {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        },
        Area {
            x: 0,
            y: 0,
            width: 2560,
            height: 1516,
        },
    ];
    // Wholly on both before; wholly on the second alone afterwards. Nothing has gone anywhere
    // the user cannot see, so nothing moves — whichever order the platform lists them in.
    assert_eq!(
        contain_growth((100, 100), (300, 310), (900, 900), &overlapping),
        None,
        "still wholly on the larger display, so the smaller one's edge is not a wall"
    );
    // Too large for every one of them, and it comes back — to the corner of the display it
    // was found on, which is the same answer `summon_position` gives an oversized board
    // (TC-125). It fits neither across nor down, so neither axis keeps its old value.
    assert_eq!(
        contain_growth((100, 100), (300, 310), (2600, 900), &overlapping),
        Some((0, 0))
    );

    // The same rule, entirely in negative coordinates.
    let left = vec![two_screens()[0]];
    let corner = summon_position(left[0], BOARD);
    assert_eq!(
        contain_growth(corner, BOARD, (525, 543), &left),
        Some((-1920 + 1920 - 525, corner.1)),
        "flush with the right edge of a display that is wholly left of the origin"
    );
}
