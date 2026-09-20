//! The settings edit: what it adds, what it refuses to touch, and what it refuses to do.
//!
//! This is the only code in the product that writes a file the user owns, so these tests are
//! mostly about the *absence* of changes. **None of them goes near a real settings file** —
//! every one builds its own text, and the two that write at all write into a temp directory.
//!
//! The requirements exercised here are FR-43 … FR-46, FR-51 and FR-53.

use std::path::{Path, PathBuf};

use mcv_board::hooks::{
    self, Installed, MARKER, Refusal, command_for, install_into, installed_in, preview, remove_from,
};
use serde_json::{Value, json};

fn helper() -> PathBuf {
    PathBuf::from(r"C:\Program Files\Monitor Claude\mcv-hook.exe")
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("the fixture is JSON")
}

/// Every handler under an event, ours and theirs.
fn handlers_of<'a>(settings: &'a Value, event: &str) -> Vec<&'a Value> {
    settings["hooks"][event]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| group["hooks"].as_array())
                .flatten()
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------------ what it adds

/// The five events of FR-47, and no sixth.
///
/// The list is written out here rather than read from `hooks::EVENTS`, because a test that
/// asks the code what it should do agrees with it by construction. These are the names from
/// `docs/hook-setup.md` §3.2, pinned against the documentation on 2026-08-27.
#[test]
fn it_installs_exactly_the_five_events() {
    let after = install_into(&json!({}), &helper()).expect("a plain object installs");

    let events: Vec<&str> = after["hooks"]
        .as_object()
        .expect("hooks is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        events,
        vec![
            "SessionStart",
            "UserPromptSubmit",
            "Notification",
            "Stop",
            "SessionEnd"
        ],
    );

    // FR-47 is a property of the list: nothing installed may be able to block or alter a
    // tool call, and every event that can do so runs *before* one.
    for forbidden in ["PreToolUse", "PreCompact", "PermissionDecision"] {
        assert!(
            after["hooks"].get(forbidden).is_none(),
            "{forbidden} runs before the thing it reports and could change the session",
        );
    }
}

/// Each entry names the helper, the event, the marker, and a timeout (FR-46, FR-48).
#[test]
fn each_entry_carries_the_marker_and_a_timeout() {
    let after = install_into(&json!({}), &helper()).expect("installs");

    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "Notification",
        "Stop",
        "SessionEnd",
    ] {
        let handlers = handlers_of(&after, event);
        assert_eq!(handlers.len(), 1, "one entry on {event}");
        let handler = handlers[0];

        assert_eq!(handler["type"], "command");
        assert_eq!(handler["timeout"], 5);

        let command = handler["command"].as_str().expect("a command string");
        assert!(command.ends_with(MARKER), "{command}");
        assert!(
            command.contains(event),
            "the entry names its own event: {command}"
        );
        assert!(
            command.starts_with('"') && command.contains("mcv-hook.exe\""),
            "the path is quoted, because it has a space in it: {command}",
        );
    }
}

// ------------------------------------------------------------------ FR-45

/// Nothing the product does not own is modified, reordered, or removed.
#[test]
fn it_touches_nothing_else() {
    let before = parse(
        r#"{
          "permissions": {"allow": ["Bash(git status)"]},
          "model": "opus",
          "hooks": {
            "PreToolUse": [
              {"matcher": "Bash", "hooks": [{"type": "command", "command": "audit.sh"}]}
            ],
            "Stop": [
              {"hooks": [{"type": "command", "command": "notify-me.sh", "timeout": 30}]}
            ]
          },
          "effortLevel": "high"
        }"#,
    );
    let after = install_into(&before, &helper()).expect("installs");

    // Every top-level key, in the order it was in. `serde_json`'s `preserve_order` feature
    // is what makes this hold; without it the file comes back alphabetised, which FR-45
    // forbids as plainly as deleting a line would.
    let keys: Vec<&str> = after
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["permissions", "model", "hooks", "effortLevel"]);
    assert_eq!(after["permissions"], before["permissions"]);
    assert_eq!(after["model"], before["model"]);
    assert_eq!(after["effortLevel"], before["effortLevel"]);

    // The user's own hook on a tool call is left exactly where it was, even though we would
    // never install one there ourselves.
    assert_eq!(after["hooks"]["PreToolUse"], before["hooks"]["PreToolUse"]);

    // And their `Stop` hook keeps both its content and its position: ours goes after it, so
    // theirs still runs first.
    let stop = handlers_of(&after, "Stop");
    assert_eq!(stop.len(), 2);
    assert_eq!(stop[0]["command"], "notify-me.sh");
    assert!(stop[1]["command"].as_str().expect("ours").ends_with(MARKER));
}

// ------------------------------------------------------------------ FR-46

/// Installing twice leaves one entry per event, not two.
#[test]
fn installing_twice_changes_nothing_the_second_time() {
    let once = install_into(&json!({}), &helper()).expect("installs");
    let twice = install_into(&once, &helper()).expect("installs again");

    assert_eq!(once, twice, "setup is idempotent");
    assert_eq!(handlers_of(&twice, "Stop").len(), 1);
}

/// An upgrade replaces the older version's entries — matched by the marker, not by the
/// command string, which is what lets the path and the arguments change between versions.
#[test]
fn an_upgrade_replaces_only_our_older_entries() {
    let before = parse(
        r#"{
          "hooks": {
            "Stop": [
              {"hooks": [{"type": "command", "command": "theirs.sh"}]},
              {"hooks": [{"type": "command",
                          "command": "\"C:\\old\\mcv-hook.exe\" Stop # monitor-claude-vscode v0"}]}
            ],
            "PostToolUse": [
              {"hooks": [{"type": "command",
                          "command": "\"C:\\old\\mcv-hook.exe\" PostToolUse # monitor-claude-vscode v0"}]}
            ]
          }
        }"#,
    );
    let after = install_into(&before, &helper()).expect("upgrades");

    let stop: Vec<&str> = handlers_of(&after, "Stop")
        .iter()
        .map(|handler| handler["command"].as_str().expect("a command"))
        .collect();
    assert_eq!(stop.len(), 2, "theirs, and exactly one of ours");
    assert_eq!(stop[0], "theirs.sh");
    assert!(stop[1].contains("v1") && stop[1].contains("Program Files"));

    // An event the old version installed and this one does not is *ours*, so it goes — and
    // the now-empty event key goes with it, rather than being left as a husk.
    assert!(
        after["hooks"].get("PostToolUse").is_none(),
        "an entry of ours under an event we no longer install is still ours to remove",
    );
}

/// What the board asks before offering anything (FR-54: never install without being asked,
/// and never re-offer what is already there).
#[test]
fn it_can_tell_whether_its_own_entries_are_there() {
    assert_eq!(installed_in(&json!({}), &helper()), Ok(Installed::No));
    assert_eq!(
        installed_in(
            &parse(r#"{"hooks":{"Stop":[{"hooks":[{"command":"theirs.sh"}]}]}}"#),
            &helper()
        ),
        Ok(Installed::No),
        "somebody else's hooks are not ours",
    );

    let installed = install_into(&json!({}), &helper()).expect("installs");
    assert_eq!(installed_in(&installed, &helper()), Ok(Installed::Current));

    // An older version, a half-removed install, and a duplicate all answer the same way,
    // because remove-then-add reaches the same end state from all three.
    let older = parse(
        r#"{"hooks":{"Stop":[{"hooks":[{"command":"old.exe Stop # monitor-claude-vscode v0"}]}]}}"#,
    );
    assert_eq!(installed_in(&older, &helper()), Ok(Installed::Other));

    let mut partial = installed.clone();
    partial["hooks"]
        .as_object_mut()
        .expect("object")
        .remove("Stop");
    assert_eq!(installed_in(&partial, &helper()), Ok(Installed::Other));

    let doubled = {
        let mut doubled = installed.clone();
        let groups = doubled["hooks"]["Stop"].as_array_mut().expect("array");
        let copy = groups[0].clone();
        groups.push(copy);
        doubled
    };
    assert_eq!(installed_in(&doubled, &helper()), Ok(Installed::Other));
}

/// A helper installed somewhere else is still ours, but is not *current* — which is what
/// makes a moved install offer to fix itself instead of claiming to be fine.
#[test]
fn a_helper_at_a_different_path_is_not_current() {
    let installed =
        install_into(&json!({}), Path::new("C:/elsewhere/mcv-hook.exe")).expect("installs");
    assert_eq!(installed_in(&installed, &helper()), Ok(Installed::Other));
}

// ------------------------------------------------------------------ FR-51

/// Removal takes out ours and leaves everything else, including the shape of the file.
#[test]
fn removal_leaves_the_file_as_it_was_found() {
    let before = parse(
        r#"{
          "model": "opus",
          "hooks": {
            "Stop": [
              {"hooks": [{"type": "command", "command": "notify-me.sh"}]}
            ]
          }
        }"#,
    );
    let installed = install_into(&before, &helper()).expect("installs");
    let removed = remove_from(&installed).expect("removes");

    assert_eq!(removed, before, "install then remove is a round trip");
}

/// Removing from a file with nothing of ours in it changes nothing at all — which is what
/// makes removal safe to offer whatever state the file is in.
#[test]
fn removal_without_our_entries_is_a_no_op() {
    let before =
        parse(r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"command":"theirs.sh"}]}]}}"#);
    assert_eq!(remove_from(&before).expect("removes"), before);

    let bare = parse(r#"{"model":"opus"}"#);
    assert_eq!(remove_from(&bare).expect("removes"), bare);
}

/// An empty `hooks` object the user left behind is theirs, not litter for us to sweep.
#[test]
fn removal_leaves_an_empty_hooks_object_that_was_already_there() {
    let before = parse(r#"{"hooks":{}}"#);
    assert_eq!(remove_from(&before).expect("removes"), before);

    let empty_event = parse(r#"{"hooks":{"Stop":[]}}"#);
    assert_eq!(remove_from(&empty_event).expect("removes"), empty_event);
}

/// A group we share with the user survives losing our handler from it.
#[test]
fn removal_keeps_a_group_that_still_has_someone_elses_entry_in_it() {
    let before = parse(&format!(
        r#"{{"hooks":{{"Stop":[{{"matcher":"*","hooks":[
             {{"type":"command","command":"theirs.sh"}},
             {{"type":"command","command":"\"C:/x/mcv-hook.exe\" Stop {MARKER}"}}
           ]}}]}}}}"#
    ));
    let after = remove_from(&before).expect("removes");

    let groups = after["hooks"]["Stop"]
        .as_array()
        .expect("the group is still there");
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]["matcher"], "*",
        "including the parts of it we never look at"
    );
    let handlers = groups[0]["hooks"].as_array().expect("handlers");
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0]["command"], "theirs.sh");
}

// ------------------------------------------------------------------ FR-53

/// A file that will not parse is not written. Not repaired, not replaced — refused.
#[test]
fn a_file_that_will_not_parse_is_refused() {
    let half_edited = r#"{"model": "opus",}"#;
    assert!(matches!(
        preview(half_edited, Some(&helper())),
        Err(Refusal::Unparseable(_)),
    ));

    // Very often this is a comma the user is halfway through fixing. Writing over it would
    // destroy work in progress, and the backup would be a backup of the broken file.
    assert!(matches!(
        preview("", Some(&helper())),
        Err(Refusal::Unparseable(_))
    ));
    assert!(matches!(
        preview("[]", Some(&helper())),
        Err(Refusal::NotAnObject)
    ));
}

/// A `hooks` section shaped in a way this build does not recognise is refused too, rather
/// than being replaced with one it does recognise.
#[test]
fn an_unexpected_shape_is_refused() {
    for (text, at) in [
        (r#"{"hooks": "off"}"#, "hooks"),
        (r#"{"hooks": {"Stop": "off"}}"#, "hooks.<event>"),
        (
            r#"{"hooks": {"Stop": [{"hooks": 3}]}}"#,
            "hooks.<event>[].hooks",
        ),
    ] {
        assert_eq!(
            preview(text, Some(&helper())).err(),
            Some(Refusal::UnexpectedShape(at)),
            "{text}",
        );
    }
}

// ------------------------------------------------------------------ FR-43

/// The preview shows the change, and the change only.
#[test]
fn the_preview_shows_what_will_be_added() {
    let before = "{\n  \"model\": \"opus\"\n}\n";
    let change = preview(before, Some(&helper())).expect("previews");

    assert!(!change.nothing_to_do);
    assert!(
        !change.reformats,
        "a two-space file is what our own printer produces, so nothing but our lines move",
    );

    let removed: Vec<&str> = change.diff.lines().filter(|l| l.starts_with('-')).collect();
    assert_eq!(
        removed,
        vec!["-  \"model\": \"opus\""],
        "the only line marked removed is the one that gains a trailing comma",
    );
    let added: Vec<&str> = change.diff.lines().filter(|l| l.starts_with('+')).collect();
    assert_eq!(
        added.iter().filter(|l| l.contains(MARKER)).count(),
        5,
        "one command line per event, every one of them visible before the user agrees: {added:#?}",
    );
    assert!(
        added.iter().any(|l| l.contains("\"timeout\": 5")),
        "including the timeout, which is a change to their machine like any other",
    );
    assert!(change.after.contains("\"model\": \"opus\""));
}

/// The change in words, counted from the diff that is shown beside it.
///
/// The screen says "53 lines added, nothing removed" because FR-45's promise is only legible
/// from a diff to a reader who knows that a removal would have been marked — and that is not
/// everyone this screen is for. The counts come from the diff itself so the two cannot
/// disagree.
#[test]
fn the_change_counts_its_own_lines() {
    let with_hooks = "{
  \"hooks\": {
    \"Stop\": [
      {
        \"hooks\": [
          {
            \"command\": \"theirs.sh\"
          }
        ]
      }
    ]
  }
}
";

    let install = preview(with_hooks, Some(&helper())).expect("previews");
    assert_eq!(
        install.added,
        install.diff.lines().filter(|l| l.starts_with('+')).count(),
    );
    assert!(install.added > 0);
    assert_eq!(
        install.removed, 0,
        "adding to a settings file that already has a hooks section removes nothing",
    );
    assert_eq!(
        install.entries_added, 5,
        "and the count is restated in the unit the user was given: five entries",
    );
    assert_eq!(install.entries_removed, 0);

    // And the other direction: removing ours takes lines out and puts none back.
    let installed = hooks::preview(with_hooks, Some(&helper()))
        .expect("previews")
        .after;
    let removal = preview(&installed, None).expect("previews");
    assert!(removal.removed > 0);
    assert_eq!(
        removal.added, 0,
        "removal is removal: it does not rewrite anything on the way out",
    );
    assert_eq!(removal.entries_removed, 5);
    assert_eq!(removal.entries_added, 0);
}

/// A file written some other way is reformatted by us, and the preview says so before the
/// user agrees rather than after.
#[test]
fn the_preview_says_when_the_file_will_be_reformatted() {
    let four_spaces = "{\n    \"model\": \"opus\"\n}\n";
    let change = preview(four_spaces, Some(&helper())).expect("previews");
    assert!(change.reformats);

    let two_spaces = "{\n  \"model\": \"opus\"\n}\n";
    assert!(
        !preview(two_spaces, Some(&helper()))
            .expect("previews")
            .reformats
    );
}

/// Removing when there is nothing of ours previews as nothing, so the flow can say so
/// instead of showing an empty diff and a backup.
#[test]
fn a_change_that_does_nothing_says_so() {
    let change = preview(r#"{"model":"opus"}"#, None).expect("previews");
    assert!(change.nothing_to_do);
    assert_eq!(change.diff.trim(), "");
}

// ------------------------------------------------------------------ FR-44, the file

/// A temp directory, never the real `~/.claude`.
struct Scratch(PathBuf);

impl Scratch {
    fn new(case: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mcv-hooks-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        Self(dir)
    }

    fn settings(&self) -> PathBuf {
        self.0.join("settings.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 2026-08-27T09:41:05Z, so the stamp in the name is checkable rather than merely present.
const WHEN: i64 = 1_787_823_665_000;

#[test]
fn the_backup_is_written_first_and_named_for_the_moment() {
    let scratch = Scratch::new("backup");
    let original = "{\n  \"model\": \"opus\"\n}\n";
    std::fs::write(scratch.settings(), original).expect("write the settings");

    let written = hooks::write(&scratch.settings(), Some(&helper()), WHEN).expect("installs");

    assert_eq!(
        written.backup.file_name().and_then(|n| n.to_str()),
        Some("settings.json.backup-20260827-094105"),
        "beside the file, so the user can find it without us",
    );
    assert_eq!(
        std::fs::read_to_string(&written.backup).expect("the backup exists"),
        original,
        "the backup is the file as it was, byte for byte",
    );

    let now = std::fs::read_to_string(scratch.settings()).expect("read back");
    assert!(now.contains(MARKER));
    assert!(now.contains("\"model\": \"opus\""));

    // A second install in the same second does not overwrite the first backup.
    let again = hooks::write(&scratch.settings(), Some(&helper()), WHEN).expect("installs again");
    assert_ne!(again.backup, written.backup);
    assert!(
        std::fs::read_to_string(&written.backup).is_ok(),
        "the first backup survives"
    );
}

/// The refusal happens before anything is touched: no backup, no write, no half-state.
#[test]
fn a_refusal_leaves_the_file_and_the_directory_alone() {
    let scratch = Scratch::new("refuse");
    let broken = "{\n  \"model\": \"opus\",\n}\n";
    std::fs::write(scratch.settings(), broken).expect("write the settings");

    let outcome = hooks::write(&scratch.settings(), Some(&helper()), WHEN);
    assert!(outcome.is_err());

    assert_eq!(
        std::fs::read_to_string(scratch.settings()).expect("still there"),
        broken,
        "the file the product could not read is the file it left behind",
    );
    let stray: Vec<_> = std::fs::read_dir(&scratch.0)
        .expect("read the directory")
        .filter_map(Result::ok)
        .filter(|e| e.file_name() != std::ffi::OsStr::new("settings.json"))
        .collect();
    assert!(
        stray.is_empty(),
        "not even a backup of the file it refused to change"
    );
}

/// No settings file yet is a normal first install, not an error — and there is nothing to
/// back up, which the empty path says.
#[test]
fn a_missing_settings_file_is_a_first_install() {
    let scratch = Scratch::new("first");
    let written = hooks::write(&scratch.settings(), Some(&helper()), WHEN).expect("installs");

    assert_eq!(written.backup, PathBuf::new());
    let now = std::fs::read_to_string(scratch.settings()).expect("written");
    assert!(now.contains(MARKER));
    assert_eq!(
        installed_in(&parse(&now), &helper()),
        Ok(Installed::Current),
    );
}

/// Removal does not depend on the backup existing, or on anything else the install left
/// behind (FR-51).
#[test]
fn removal_works_from_the_file_alone() {
    let scratch = Scratch::new("remove");
    let original = "{\n  \"model\": \"opus\"\n}\n";
    std::fs::write(scratch.settings(), original).expect("write");

    let installed = hooks::write(&scratch.settings(), Some(&helper()), WHEN).expect("installs");
    std::fs::remove_file(&installed.backup).expect("throw the backup away");

    hooks::write(&scratch.settings(), None, WHEN + 1000).expect("removes");
    assert_eq!(
        std::fs::read_to_string(scratch.settings()).expect("read back"),
        original,
        "back to the file it started as",
    );
}

/// The command string is what the setup writes into the file, and what the helper is later
/// invoked with (`docs/hook-setup.md` §3.2).
#[test]
fn the_command_is_the_one_the_documentation_shows() {
    assert_eq!(
        command_for("Notification", Path::new("C:/x/mcv-hook.exe")),
        "\"C:/x/mcv-hook.exe\" Notification # monitor-claude-vscode v1",
    );
}
