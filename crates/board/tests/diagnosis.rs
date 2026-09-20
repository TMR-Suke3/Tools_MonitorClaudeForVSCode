//! The diagnosis panel — FR-55's second half, `docs/hook-setup.md` §5.
//!
//! Four preconditions, each with the single action that repairs it. What is asserted here is
//! that shape, not the prose: that a broken row carries exactly one action, that the action is
//! the *right kind*, and — the case worth the most — that a row offers nothing when it holds
//! and nothing when the repair could not work.
//!
//! **None of these touches the machine's own hook setup.** `setup::diagnose` is a function of
//! `Facts`, which is why it was split out of `hook_diagnosis`: the four situations this panel
//! exists to describe cannot be produced on the developer's machine without breaking that
//! machine's real settings file, and a test that did so would be worse than no test.

use std::path::{Path, PathBuf};

use mcv_board::events::QUIET_AFTER_MILLIS;
use mcv_board::hooks::Installed;
use mcv_board::setup::{Check, Diagnosis, Facts, SettingsFile, diagnose};

/// Some fixed instant. Nothing here depends on the wall clock.
const NOW: i64 = 1_800_000_000_000;

const SETTINGS: &str = r"C:\Users\someone\.claude\settings.json";

fn helper() -> PathBuf {
    PathBuf::from(r"C:\Program Files\Monitor Claude\mcv-hook.exe")
}

/// Everything working: the helper is there, the file reads, our entries are in it, and an
/// event arrived a minute ago.
fn working() -> (PathBuf, SettingsFile) {
    (helper(), SettingsFile::Read(Installed::Current))
}

fn run(helper_at: &Path, found: bool, settings: &SettingsFile, last: Option<i64>) -> Diagnosis {
    diagnose(&Facts {
        helper: Some(helper_at),
        helper_found: found,
        settings_path: SETTINGS,
        settings,
        last_event: last,
        now: NOW,
    })
}

fn row<'a>(diagnosis: &'a Diagnosis, id: &str) -> &'a Check {
    diagnosis
        .checks
        .iter()
        .find(|check| check.id == id)
        .unwrap_or_else(|| panic!("the panel has no {id} row"))
}

fn kind(check: &Check) -> Option<&str> {
    check.action.as_ref().map(|action| action.kind)
}

// ------------------------------------------------------- the shape of the panel

/// TC-103. The four preconditions of `hook-setup.md` §5, in repair order.
///
/// The order is the assertion. The helper comes first because nothing below it can be fixed
/// while it is missing, and the file comes before what is in it for the same reason — a panel
/// that listed them the other way round would send the user at the last problem first.
#[test]
fn it_reports_four_preconditions_in_repair_order() {
    let (path, settings) = working();
    let diagnosis = run(&path, true, &settings, Some(NOW - 60_000));

    let ids: Vec<&str> = diagnosis.checks.iter().map(|check| check.id).collect();
    assert_eq!(ids, vec!["helper", "settings", "entries", "events"]);
}

/// TC-103, the other half. **One action, never two.** FR-55 asks for *the single action that
/// repairs it*, and a row with a choice in it has stopped being that. `Option` makes the "never
/// two" free; what this asserts is the other half — that a row which holds offers nothing at all,
/// because a repair button next to a green tick is how a panel stops being read.
#[test]
fn a_row_that_holds_offers_nothing() {
    let (path, settings) = working();
    let diagnosis = run(&path, true, &settings, Some(NOW - 60_000));

    for check in &diagnosis.checks {
        assert!(check.ok, "{} should hold when everything works", check.id);
        assert!(
            check.action.is_none(),
            "{} holds, so it must not offer a repair",
            check.id
        );
    }
    assert_eq!(diagnosis.mode, "exact");
}

/// Every row says something specific. "Something is wrong" is what the user already knew.
#[test]
fn every_row_names_the_precondition_and_says_what_is_true() {
    let (path, settings) = working();
    for check in run(&path, true, &settings, Some(NOW - 60_000)).checks {
        assert!(!check.what.is_empty(), "{} has no title", check.id);
        assert!(!check.detail.is_empty(), "{} says nothing", check.id);
    }
}

// ------------------------------------------------------------- the four failures

/// TC-103b. The one thing the product cannot repair. It gets a sentence, not a button: a control
/// that could only report its own failure is worse than saying so plainly.
#[test]
fn a_missing_helper_is_told_rather_than_offered_a_button() {
    let diagnosis = run(&helper(), false, &SettingsFile::Read(Installed::No), None);

    let helper_row = row(&diagnosis, "helper");
    assert!(!helper_row.ok);
    assert_eq!(kind(helper_row), Some("tell"));
    assert!(
        helper_row.detail.contains("mcv-hook"),
        "it should say where the program is not: {:?}",
        helper_row.detail
    );
}

/// TC-103c. **The trap this row exists to avoid.** Setting up while the helper is missing writes
/// five entries naming a program that is not there — and `hook_preview` refuses it anyway. So the
/// entries row states what is true and leaves the row above it to be the one acted on.
#[test]
fn the_entries_offer_no_repair_while_the_helper_is_missing() {
    let diagnosis = run(&helper(), false, &SettingsFile::Read(Installed::No), None);

    let entries = row(&diagnosis, "entries");
    assert!(!entries.ok);
    assert_eq!(
        kind(entries),
        None,
        "a repair that is going to be refused must not be offered"
    );
}

/// TC-103d. FR-53: a file that cannot be read is not written to, and the one useful action is to
/// open it. The row below it does not offer a second, contradictory action.
#[test]
fn an_unreadable_settings_file_offers_only_opening_it() {
    let unreadable = SettingsFile::Unreadable("trailing comma at line 4".to_owned());
    let diagnosis = run(&helper(), true, &unreadable, None);

    let settings = row(&diagnosis, "settings");
    assert!(!settings.ok);
    assert_eq!(kind(settings), Some("open-settings"));
    assert!(
        settings.detail.contains(SETTINGS),
        "the row should name the file: {:?}",
        settings.detail
    );

    let entries = row(&diagnosis, "entries");
    assert_eq!(
        kind(entries),
        None,
        "nothing can be said about entries in a file that cannot be read"
    );
}

/// TC-103d, the other half. A settings file that does not exist yet is **not** a fault: a first
/// install writes the file Claude Code would have written itself. Marking it broken would send the
/// user off to open a file that is not there, while the row below already offers the thing that
/// creates it.
#[test]
fn a_missing_settings_file_is_not_a_fault() {
    let diagnosis = run(&helper(), true, &SettingsFile::Missing, None);

    let settings = row(&diagnosis, "settings");
    assert!(settings.ok, "not there yet is an ordinary first run");
    assert_eq!(kind(settings), None);

    let entries = row(&diagnosis, "entries");
    assert!(!entries.ok);
    assert_eq!(kind(entries), Some("install"));
}

/// An older install, or a partial removal. Repairable, and by the same action as a first
/// install — remove-then-add reaches the same end state from all of them (FR-46).
#[test]
fn entries_that_are_ours_but_not_this_set_are_repairable() {
    let diagnosis = run(&helper(), true, &SettingsFile::Read(Installed::Other), None);

    let entries = row(&diagnosis, "entries");
    assert!(!entries.ok);
    assert_eq!(kind(entries), Some("install"));
}

// ------------------------------------------------------------------- the silence

/// FR-52's case, and the row the other three exist for. Entries in place and nothing arriving
/// is exactly what a panel that stopped at "installed" would report as success.
#[test]
fn installed_and_silent_is_a_failure_with_an_action() {
    let long_ago = NOW - QUIET_AFTER_MILLIS - 1;
    let diagnosis = run(
        &helper(),
        true,
        &SettingsFile::Read(Installed::Current),
        Some(long_ago),
    );

    assert_eq!(diagnosis.mode, "went-quiet");
    let events = row(&diagnosis, "events");
    assert!(!events.ok);
    assert_eq!(kind(events), Some("watch"));

    // The three rows above it all hold, which is the whole difficulty this panel solves: the
    // board is not exact, and nothing that can be checked statically is wrong.
    for id in ["helper", "settings", "entries"] {
        assert!(row(&diagnosis, id).ok, "{id} should still hold");
    }
}

/// TC-103e. Silence with **no entries installed** is not a fault to act on — it is what anyone
/// would expect, and the action belongs to the entries row above. Two rows offering the same repair
/// is how "the single action" stops being single.
#[test]
fn silence_with_nothing_installed_offers_no_action_of_its_own() {
    let diagnosis = run(&helper(), true, &SettingsFile::Read(Installed::No), None);

    let events = row(&diagnosis, "events");
    assert!(!events.ok);
    assert_eq!(kind(events), None);
    assert_eq!(
        kind(row(&diagnosis, "entries")),
        Some("install"),
        "the repair for this situation belongs to the row above"
    );
}

/// TC-104. **The event log outlives the entries that filled it.** Take them out and a report from a
/// minute ago is still sitting in the file, so a row that only looked at the newest timestamp
/// would say the reporting is fine on the same screen as "none of them are there".
#[test]
fn a_leftover_event_log_is_not_evidence_that_anything_is_reporting() {
    let diagnosis = run(
        &helper(),
        true,
        &SettingsFile::Read(Installed::No),
        // Well inside the window that would otherwise read as exact.
        Some(NOW - 60_000),
    );

    let events = row(&diagnosis, "events");
    assert!(
        !events.ok,
        "nothing can be reporting in with no entries in place: {:?}",
        events.detail
    );
    assert_eq!(
        kind(events),
        None,
        "the repair for this belongs to the entries row"
    );
    assert_eq!(diagnosis.mode, "inferred");
}

/// TC-104b. The threshold is the core's, not a second copy of it: the same silence that drops a
/// session back to inference is the silence that fails this row (FR-52).
#[test]
fn the_silence_threshold_is_the_one_the_board_uses() {
    let settings = SettingsFile::Read(Installed::Current);
    let just_inside = run(&helper(), true, &settings, Some(NOW - QUIET_AFTER_MILLIS));
    assert!(row(&just_inside, "events").ok);
    assert_eq!(just_inside.mode, "exact");

    let just_outside = run(
        &helper(),
        true,
        &settings,
        Some(NOW - QUIET_AFTER_MILLIS - 1),
    );
    assert!(!row(&just_outside, "events").ok);
    assert_eq!(just_outside.mode, "went-quiet");
}

/// TC-104b, the literal. Thirty minutes, as `session-state-model.md` states it. Written out here
/// rather than read from the constant, because a test that asks the code what the number should be
/// agrees with it by construction.
#[test]
fn the_silence_threshold_is_thirty_minutes() {
    assert_eq!(QUIET_AFTER_MILLIS, 30 * 60 * 1_000);
}
