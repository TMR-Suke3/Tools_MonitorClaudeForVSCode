//! What the board remembers, and where it comes back (FR-28, TC-68).
//!
//! `clamp_to_displays` is the part of TC-68 that can be tested without unplugging a monitor:
//! given a remembered position and the displays that exist now, where does the board open?
//! The rest of that case — that the display really is gone — needs a hand and a cable.

use mcv_board::settings::{Area, Settings, clamp_to_displays};

const PRIMARY: Area = Area {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};
/// A second monitor to the left, which is where a board often ends up.
const SECOND: Area = Area {
    x: -1920,
    y: 0,
    width: 1920,
    height: 1080,
};
const BOARD: (u32, u32) = (300, 200);

#[test]
fn a_position_that_still_fits_is_left_exactly_where_it_was() {
    // 800 + 200 fits inside 1080; 900 would not, and the clamp would be right to move it.
    assert_eq!(
        clamp_to_displays((1500, 800), BOARD, &[PRIMARY]),
        Some((1500, 800)),
        "the board must not drift on every restart",
    );
    assert_eq!(
        clamp_to_displays((-1800, 40), BOARD, &[PRIMARY, SECOND]),
        Some((-1800, 40)),
        "a second display is a place the board is allowed to be",
    );
}

#[test]
fn a_vanished_display_brings_the_board_back_to_the_nearest_one() {
    // Remembered on the left-hand monitor, which is no longer plugged in.
    let landed = clamp_to_displays((-1800, 40), BOARD, &[PRIMARY]).expect("somewhere to go");

    assert!(
        landed.0 >= PRIMARY.x && landed.1 >= PRIMARY.y,
        "the board came back off-screen at {landed:?}",
    );
    assert!(
        landed.0 + BOARD.0 as i32 <= PRIMARY.width as i32,
        "the board came back hanging off the right edge at {landed:?}",
    );
    assert_eq!(
        landed.1, 40,
        "the vertical position was reachable and should have been kept",
    );
}

#[test]
fn a_board_hanging_off_an_edge_is_pushed_fully_inside() {
    let landed = clamp_to_displays((1900, 1000), BOARD, &[PRIMARY]).expect("somewhere to go");
    assert_eq!(
        landed,
        (1920 - 300, 1080 - 200),
        "a board half off the screen is a board the user cannot drag back",
    );
}

#[test]
fn with_no_displays_at_all_the_platform_decides() {
    assert_eq!(
        clamp_to_displays((10, 10), BOARD, &[]),
        None,
        "there is nothing to clamp to, and inventing a position would be worse",
    );
}

#[test]
fn fold_state_is_remembered_per_group_and_forgotten_when_unfolded() {
    let mut settings = Settings::default();
    assert!(
        !settings.is_folded("C:/work/sample-repo"),
        "a workspace the board has never seen starts open",
    );

    assert!(settings.set_folded("C:/work/sample-repo", true));
    assert!(settings.is_folded("C:/work/sample-repo"));
    assert!(
        !settings.is_folded("C:/work/other-repo"),
        "one group at a time"
    );

    assert!(settings.set_folded("C:/work/sample-repo", false));
    assert!(
        settings.folded.is_empty(),
        "an unfolded group leaves nothing behind"
    );
}

#[test]
fn the_file_survives_a_round_trip_and_a_corrupt_one_is_not_fatal() {
    let dir = std::env::temp_dir().join(format!("mcv-settings-{}", std::process::id()));
    let path = dir.join("board.json");

    let mut settings = Settings {
        position: Some((-1800, 40)),
        ..Default::default()
    };
    settings.set_folded("C:/work/other-repo", true);
    settings.save(&path).expect("saved");

    assert_eq!(Settings::load(&path), settings, "what went in comes back");

    std::fs::write(&path, "{ this is not json").expect("write");
    assert_eq!(
        Settings::load(&path),
        Settings::default(),
        "a corrupt file starts the board where it would have started anyway, rather than \
         refusing to open",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Nothing about a session is written to disk (NFR-07). The board remembers where it was and
/// what was folded, how big to draw itself, which two keys to listen for, and whether a click
/// opens the session's own tab; a workspace key is the most it keeps, and only for groups the
/// user folded.
///
/// The list is written out here rather than derived from `Settings`, because a test that asks
/// the type what it should contain agrees with it by construction. It is FR-28's own list,
/// and a field added without amending FR-28 fails here — which is what this is for.
#[test]
fn nothing_but_position_and_fold_state_is_written() {
    let mut settings = Settings {
        position: Some((100, 200)),
        ..Default::default()
    };
    settings.set_folded("C:/work/sample-repo", true);

    let json = serde_json::to_string(&settings).expect("serialised");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parsed");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    assert_eq!(
        keys,
        vec!["folded", "hotkeys", "position", "reveal_session", "scale"],
        "the settings file holds exactly what FR-28 lists, and none of it is content",
    );

    // The point of the case, stated so that adding a field cannot quietly pass it: whatever
    // the board writes down, none of it may say what the user is working on. A workspace key
    // under `folded` is the most any entry is allowed to reveal, and the title of a session,
    // a prompt, or a path is never written (NFR-07).
    assert!(
        !json.contains("sample-repo/") && !json.to_lowercase().contains("prompt"),
        "the file grew something that looks like content: {json}",
    );
}

/// A size the board does not draw at is not a size the board draws at.
///
/// The file can say anything — hand-edited, or written by a build that offered a step this
/// one does not — and a board drawn at 137 % would be a board whose folded width was measured
/// for a size it is not.
#[test]
fn a_scale_the_board_does_not_offer_falls_back() {
    let mut settings = Settings::default();
    assert_eq!(
        settings.scale(),
        100,
        "the default is the design's own size"
    );

    for percent in mcv_board::view::SCALES {
        settings.scale = percent;
        assert_eq!(settings.scale(), percent, "every offered step is kept");
    }

    for odd in [0, 37, 137, 1000] {
        settings.scale = odd;
        assert_eq!(
            settings.scale(),
            100,
            "{odd} % is not a size this board draws at",
        );
    }
}

/// The shortcuts have defaults, and they are the ones the menu shows.
#[test]
fn the_shortcuts_have_defaults() {
    let settings = Settings::default();
    assert_eq!(settings.hotkeys.summon, "Ctrl+Alt+M");
    assert_eq!(settings.hotkeys.toggle, "Ctrl+Alt+B");
    assert_ne!(
        settings.hotkeys.summon, settings.hotkeys.toggle,
        "two keys for two things",
    );
}
