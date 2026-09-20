//! TC-100 — every built-in command the front end calls is one the front end is allowed to
//! call.
//!
//! Tauri v2 denies every core command that no capability grants, and the denial happens in
//! the front end, where nothing is watching: `invoke` rejects a promise that no one awaited
//! and the window carries on looking fine. The board shipped a whole phase that way — it
//! rendered once from `current_board`, never received a `board` event again, and could not
//! be dragged. Nothing failed; it simply stopped being a live board.
//!
//! Commands this product defines itself need no grant — registering them is the grant — so
//! `current_board`, `toggle_fold` and `raise_session` all worked, which is exactly why the
//! failure looked like a rendering bug rather than a permission one.
//!
//! This case reads the capability file and the front end and compares them. It is a
//! configuration test, and it exists because the configuration is the part that failed.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn board_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = board_dir().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn capability() -> serde_json::Value {
    serde_json::from_str(&read("capabilities/default.json")).expect("the capability file is JSON")
}

fn granted() -> BTreeSet<String> {
    capability()["permissions"]
        .as_array()
        .expect("permissions is a list")
        .iter()
        .map(|p| p.as_str().expect("a permission is a string").to_owned())
        .collect()
}

/// The front end, as one string. Both windows, because a capability covers both.
fn front_end() -> String {
    format!("{}{}", read("ui/board.js"), read("ui/setup.js"))
}

/// What the front end calls, and what each call needs granted.
///
/// The left column is a fragment of the JavaScript that makes the call; the right column is
/// the permission without which it fails. Adding a built-in call to the UI means adding a row
/// here — which is the point: the row is where somebody notices the permission is needed.
const NEEDED: [(&str, &str); 2] = [
    ("event.listen(", "core:event:allow-listen"),
    ("startDragging", "core:window:allow-start-dragging"),
];

/// TC-100 (FR-18, FR-27) — the calls the front end makes are granted.
#[test]
fn tc_100_every_built_in_call_the_front_end_makes_is_granted() {
    let source = front_end();
    let granted = granted();

    for (call, permission) in NEEDED {
        if !source.contains(call) {
            // The call was removed. Nothing to grant — but say so, rather than passing
            // quietly on a table that has drifted away from the code it describes.
            panic!(
                "the front end no longer calls `{call}`; if that is deliberate, drop its row \
                 from NEEDED and consider dropping `{permission}` from the capability",
            );
        }
        assert!(
            granted.contains(permission),
            "the front end calls `{call}`, which Tauri denies without `{permission}`. \
             A denied core command fails silently in the webview: this is the shape of the \
             bug where the board rendered once and never updated again",
        );
    }
}

/// Nothing is granted that nothing asks for.
///
/// The same restraint the hook layer takes with settings entries (FR-47): the smallest set
/// that does the job. A permission nobody can point at a caller for is one that outlives the
/// reason it was added.
#[test]
fn tc_100b_nothing_is_granted_that_the_front_end_does_not_need() {
    // `unlisten` has no call of its own in the source: it is what the handle returned by
    // `listen` does when a listener is dropped, and denying it would strand listeners in the
    // webview's registry. It is granted deliberately, and named here so that it is not
    // mistaken for an unexplained grant.
    let explained: BTreeSet<String> = NEEDED
        .iter()
        .map(|(_, permission)| (*permission).to_owned())
        .chain(std::iter::once("core:event:allow-unlisten".to_owned()))
        .collect();

    let granted = granted();
    let unexplained: Vec<&String> = granted.difference(&explained).collect();
    assert!(
        unexplained.is_empty(),
        "granted without a caller to point at: {unexplained:?}. Add the call to NEEDED, or \
         take the permission out",
    );
}

/// Every window the product opens is covered.
///
/// A capability applies to the windows it lists. The board is declared in `tauri.conf.json`;
/// the setup window is built at runtime by `setup::open_hook_setup`, and a window created
/// later is exactly the kind that gets left off a list written earlier.
#[test]
fn tc_100c_both_windows_are_covered() {
    let windows: BTreeSet<String> = capability()["windows"]
        .as_array()
        .expect("windows is a list")
        .iter()
        .map(|w| w.as_str().expect("a label is a string").to_owned())
        .collect();

    let declared = read("tauri.conf.json");
    assert!(
        declared.contains("\"label\": \"board\""),
        "the board window's label moved; this test is comparing against the wrong name",
    );
    let runtime = read("src/setup.rs");
    assert!(
        runtime.contains("const LABEL: &str = \"hook-setup\";"),
        "the setup window's label moved; this test is comparing against the wrong name",
    );

    for label in ["board", "hook-setup"] {
        assert!(
            windows.contains(label),
            "the `{label}` window is not covered by the capability, so every built-in command \
             it calls is denied",
        );
    }
}
