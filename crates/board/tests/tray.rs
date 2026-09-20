//! The tray icon (FR-32) — TC-102 … TC-102d, checked as pixels rather than as a picture.
//!
//! Everything here is about [`mcv_board::tray::pixels`], which is the only part of the icon
//! that can be wrong in a way anybody would notice: the shell, the click handling and the
//! menu are Tauri's, and the menu is the board's own and is tested where it lives.
//!
//! The claim under test is the one [ADR-0025] makes — that the icon is the **board** in
//! miniature, and so shows the pair of colours §3.3 measured, on a tile that separates itself
//! from an unmeasured taskbar. Three things follow from that and are asserted below: the
//! centre is the indicator colour, the ground under it is the board's own, and the outermost
//! ring of the tile is the edge colour rather than nothing.
//!
//! [ADR-0025]: ../../../docs/adr/0025-the-tray-icon-is-the-board-in-miniature.md

use mcv_board::tray::{SIZE, pixels, tooltip};
use mcv_core::color::Rgb;
use mcv_core::palette::{Status, Theme, indicator};

/// Three colours far enough apart that no assertion below can pass by accident.
const PANEL: Rgb = [17, 18, 20];
const EDGE: Rgb = [90, 92, 96];
const MARK: Rgb = [240, 160, 40];

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
}

/// Whether a pixel is that colour, opaque, allowing for the rounding out of the compositor.
fn is(pixel: [u8; 4], colour: Rgb) -> bool {
    pixel[3] > 250 && (0..3).all(|c| i32::from(pixel[c]).abs_diff(i32::from(colour[c])) <= 1)
}

#[test]
fn the_icon_is_the_size_it_says_it_is() {
    let rgba = pixels(PANEL, EDGE, MARK, false);
    assert_eq!(
        rgba.len(),
        (SIZE * SIZE * 4) as usize,
        "four bytes a pixel over a {SIZE} x {SIZE} square"
    );
}

/// TC-102. The centre carries the status. This is the whole feature: a hidden board still answers
/// "is anyone waiting for me?" and the colour is how it answers.
#[test]
fn the_centre_is_the_indicator_colour() {
    let rgba = pixels(PANEL, EDGE, MARK, false);
    let middle = SIZE / 2;
    assert!(
        is(pixel(&rgba, middle, middle), MARK),
        "the middle of the icon should be the mark, not {:?}",
        pixel(&rgba, middle, middle)
    );
}

/// TC-102, the other half. ADR-0025's actual claim: the mark sits on **the board's own
/// background**, which is the one
/// surface §3.3 measured every indicator against. A mark drawn straight onto a transparent
/// icon would sit on the taskbar instead, whose colour nobody has measured anything against.
#[test]
fn the_mark_sits_on_the_boards_own_background() {
    let rgba = pixels(PANEL, EDGE, MARK, false);
    // Just inside the tile and well outside the mark: a quarter of the way in diagonally,
    // which no radius here reaches.
    let ground = pixel(&rgba, 5, 5);
    assert!(
        is(ground, PANEL),
        "the ground under the mark should be the board's own colour, not {ground:?}"
    );
}

/// TC-102b. The second job of the edge: the tile has to separate itself from a taskbar whose colour
/// is the user's to choose, and which may be near enough to the board's own to vanish against.
#[test]
fn the_tile_has_an_edge_all_the_way_round() {
    let rgba = pixels(PANEL, EDGE, MARK, false);
    // The middle of each side, one pixel in from the icon's bounds — where the outline is.
    for (x, y) in [
        (SIZE / 2, 1),
        (SIZE / 2, SIZE - 2),
        (1, SIZE / 2),
        (SIZE - 2, SIZE / 2),
    ] {
        let found = pixel(&rgba, x, y);
        assert!(
            is(found, EDGE),
            "({x}, {y}) should be the board's edge colour, not {found:?}"
        );
    }
}

/// The corners are rounded, so the icon reads as the board rather than as a square swatch.
#[test]
fn the_corners_are_transparent() {
    let rgba = pixels(PANEL, EDGE, MARK, false);
    for (x, y) in [(0, 0), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
        assert_eq!(
            pixel(&rgba, x, y)[3],
            0,
            "the very corner ({x}, {y}) is outside the tile and must not be painted"
        );
    }
}

/// TC-102c. `unknown` is drawn as a ring, and the hole is what makes it one. The board draws a
/// *dotted* ring; at 16 px the dots are sub-pixel and resolve to a smudge that reads as a filled
/// disc — which would claim a definite status where the meaning is that there is not one.
#[test]
fn unknown_is_a_ring_with_a_hole_in_it() {
    let rgba = pixels(PANEL, EDGE, MARK, true);
    let middle = SIZE / 2;
    let centre = pixel(&rgba, middle, middle);
    assert!(
        is(centre, PANEL),
        "the middle of a ring is the board showing through, not {centre:?}"
    );
    // Six pixels out is between the hole and the outer radius: the ring itself.
    let on_the_ring = pixel(&rgba, middle + 6, middle);
    assert!(
        is(on_the_ring, MARK),
        "the ring should be the mark colour, not {on_the_ring:?}"
    );
}

/// TC-102d. The six statuses must not resolve to the same icon. This does not re-measure the
/// palette — TC-49 … TC-51 do that — it checks that the icon actually *uses* it, in both themes.
#[test]
fn every_status_draws_a_different_icon() {
    for theme in Theme::ALL {
        let mut seen: Vec<(Status, Vec<u8>)> = Vec::new();
        for status in Status::ALL {
            let drawn = pixels(
                theme.board(),
                theme.edge(),
                indicator(status, theme),
                matches!(status, Status::Unknown),
            );
            for (other, previous) in &seen {
                assert_ne!(
                    *previous,
                    drawn,
                    "{} and {} draw the same tray icon in the {} theme",
                    other.name(),
                    status.name(),
                    theme.name()
                );
            }
            seen.push((status, drawn));
        }
    }
}

/// Every status says something, and says it before it says what product it is: the answer is
/// why anyone is hovering.
#[test]
fn every_status_has_a_tooltip_that_leads_with_the_answer() {
    for status in Status::ALL {
        let line = tooltip(status);
        assert!(
            line.ends_with(" — Claude session board"),
            "{} should name the product last: {line:?}",
            status.name()
        );
        assert!(
            !line.starts_with(" —"),
            "{} says nothing before the product name",
            status.name()
        );
    }
}

// ------------------------------------------------------------------ the menu's tick

/// The tick on *Notice waiting instantly…* and the statement above it are the same fact.
///
/// They sit three items apart in the menu, which is far enough that a change to one and not
/// the other would look right to whoever made it. `is_exact` is the one place that decides,
/// and this is what says so.
#[test]
fn the_tick_and_the_mode_line_cannot_disagree() {
    assert!(mcv_board::menu::is_exact("exact"));
    assert!(!mcv_board::menu::is_exact("went-quiet"));
    assert!(!mcv_board::menu::is_exact("inferred"));
}

/// Installed but silent is not ticked.
///
/// FR-52: a hook layer that has stopped firing must not keep presenting an exactness it is
/// no longer earning. A tick that stayed on while the board had fallen back to inference
/// would be exactly that claim, made in the one place a user looks to check.
#[test]
fn a_hook_layer_that_went_quiet_is_not_ticked() {
    assert!(
        !mcv_board::menu::is_exact("went-quiet"),
        "installed and silent is inference, and the menu must not say otherwise",
    );
}
