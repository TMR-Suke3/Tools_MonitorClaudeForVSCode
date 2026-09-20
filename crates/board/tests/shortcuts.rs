//! The two keys, and what the board says about them (FR-61, TC-130 … TC-137).
//!
//! The defect these were written for is the one where the product argues with its owner. A key
//! this product picked can already belong to something else on the machine — the machine was
//! somebody's before it was ours — and until 2026-09-06 there was no way to change it, one
//! failure was reported against both keys, and a key nobody could parse silently disabled the
//! other one as well.
//!
//! What can be tested here is the half that is decidable by looking at the string, plus every
//! sentence the menu builds from the result. **Whether a key is free cannot be**: that is a
//! fact about the machine the tests are running on, and the product answers it by asking the
//! operating system rather than by reasoning — which is the whole design
//! ([ADR-0033](../../../docs/adr/0033-a-key-that-is-taken-is-a-choice-to-undo.md)).

use mcv_board::hotkeys::{Bound, Reading, read};
use mcv_board::menu::shortcut_label;
use mcv_board::settings::{Hotkeys, Settings};

// TC-130 — a cleared key is a decision, not a failure.
#[test]
fn a_blank_key_is_cleared_rather_than_broken() {
    assert_eq!(read(""), Reading::Cleared);
    assert_eq!(read("   "), Reading::Cleared);
    // And it reads that way to a person, too: nothing here says anything is wrong.
    let line = shortcut_label("Show or hide", "", Bound::Off);
    assert!(line.contains("not set"), "{line}");
    assert!(
        !line.contains("—"),
        "a cleared key states no problem: {line}"
    );
}

// TC-131 — the two keys are independent. One that cannot be parsed used to return before the
// other had been offered at all, so a typo in one setting silently disabled both.
#[test]
fn one_unreadable_key_says_nothing_about_the_other() {
    assert_eq!(read("Ctrl+Alt+M"), Reading::Key);
    assert_eq!(read("Ctrl+Alt+Nonsense"), Reading::Unreadable);

    // The menu says which is which, one line each.
    assert_eq!(
        shortcut_label("Bring the board to me", "Ctrl+Alt+M", Bound::Live),
        "Bring the board to me    Ctrl+Alt+M"
    );
    let broken = shortcut_label("Show or hide", "Ctrl+Alt+Nonsense", Bound::Unreadable);
    assert!(broken.contains("not a key this can use"), "{broken}");
}

// TC-132 — an accelerator that parses, and one that does not, on either side of the same line.
#[test]
fn what_counts_as_a_key() {
    for good in ["Ctrl+Alt+M", "Ctrl+Shift+F5", "Alt+Space", "Super+B"] {
        assert_eq!(read(good), Reading::Key, "{good} is a key combination");
    }
    for bad in ["Ctrl+", "+", "Ctrl+Alt+Wheel", "Hyper+Q"] {
        assert_eq!(read(bad), Reading::Unreadable, "{bad} is not one");
    }
}

// TC-133 — a key another application holds is reported as **taken**, and the difference from
// "cleared" is the point: one is somebody else's doing and one is the user's own.
#[test]
fn a_key_somebody_else_has_reads_differently_from_one_nobody_wanted() {
    let taken = shortcut_label("Show or hide", "Ctrl+Alt+B", Bound::Taken);
    let off = shortcut_label("Show or hide", "Ctrl+Alt+B", Bound::Off);

    assert!(
        taken.contains("another application has it"),
        "the reason is named, so the user knows it is not the board that failed: {taken}"
    );
    assert!(
        taken.contains("Ctrl+Alt+B"),
        "and the key is still shown, because it is what they pressed: {taken}"
    );
    assert_ne!(taken, off);
    assert!(
        !off.contains("Ctrl+Alt+B"),
        "a cleared key has no key to show: {off}"
    );
}

// TC-134 — a rebinding survives the file, and clearing one is a state the file can hold.
#[test]
fn changed_keys_are_remembered() {
    let mut settings = Settings::default();
    assert_eq!(settings.hotkeys, Hotkeys::default());

    settings.hotkeys.summon = "Ctrl+Shift+F9".to_owned();
    settings.hotkeys.toggle = String::new();

    let file =
        std::env::temp_dir().join(format!("mcv-board-shortcuts-{}.json", std::process::id()));
    settings.save(&file).expect("the settings file is written");
    let read_back = Settings::load(&file);
    std::fs::remove_file(&file).ok();

    assert_eq!(read_back.hotkeys.summon, "Ctrl+Shift+F9");
    assert_eq!(read_back.hotkeys.toggle, "");
    assert_eq!(read(&read_back.hotkeys.toggle), Reading::Cleared);
    assert_eq!(read(&read_back.hotkeys.summon), Reading::Key);
}

// TC-135 — a bare key is refused by the back end, not only by the window where it is chosen.
//
// Measured: `M`, `F5`, `` ` `` and `Escape` all parse as perfectly good accelerators. Offering
// one to the operating system takes that key from every program on the machine for as long as
// the board runs, so the rule that a shortcut needs a modifier has to live where the key is
// offered — the settings file can be edited by hand, copied between machines, or written by an
// older build, and none of those go past the setup window's JavaScript.
#[test]
fn a_key_with_no_modifier_is_not_one_this_will_take() {
    for bare in ["M", "F5", "`", "Escape", "Space", "1"] {
        assert_eq!(
            read(bare),
            Reading::Unreadable,
            "{bare} parses, and must still be refused"
        );
    }
    for held in ["Ctrl+M", "Alt+F5", "Shift+F5", "Super+B", "Ctrl+Alt+M"] {
        assert_eq!(read(held), Reading::Key, "{held} has a modifier");
    }
}

// TC-136 — both rows set to the same key is the board's own doing, and is said so.
//
// The second registration fails, and calling that *taken* would send the user hunting through
// their own machine for the application holding a key this product is holding itself.
#[test]
fn the_other_shortcut_having_it_is_not_another_application_having_it() {
    let duplicate = shortcut_label("Show or hide", "Ctrl+Alt+M", Bound::Duplicate);
    let taken = shortcut_label("Show or hide", "Ctrl+Alt+M", Bound::Taken);

    assert!(
        duplicate.contains("other shortcut"),
        "the culprit is named, and it is us: {duplicate}"
    );
    assert!(
        !duplicate.contains("another application"),
        "and it does not send them looking elsewhere: {duplicate}"
    );
    assert!(
        duplicate.contains("Ctrl+Alt+M"),
        "the key is still shown: {duplicate}"
    );
    assert_ne!(
        duplicate, taken,
        "two different states, two different lines"
    );
}

// TC-137 — the flags that open a screen mean the same thing whether or not a board is already
// running.
//
// One board per machine (FR-60) sends a second launch to the running one, and the request it
// carries is its arguments. Dropping them made `--shortcuts` — the documented way to reach
// this very screen — summon the board and open nothing, for everybody whose board was already
// open. Both paths read this mapping, so neither can drift.
#[test]
fn a_flag_asks_for_the_same_screen_from_either_direction() {
    use mcv_board::setup::{Screen, screen_for_flag};

    assert_eq!(screen_for_flag("--setup"), Some(Screen::Offer));
    assert_eq!(screen_for_flag("--diagnose"), Some(Screen::Diagnosis));
    assert_eq!(screen_for_flag("--reveal-setup"), Some(Screen::Reveal));
    assert_eq!(screen_for_flag("--shortcuts"), Some(Screen::Shortcuts));

    // Everything else is a plain second launch, and is answered with a summons.
    for other in ["--demo", "--light", "--zoom", "", "shortcuts", "--palette"] {
        assert_eq!(screen_for_flag(other), None, "{other} opens no screen");
    }
}
