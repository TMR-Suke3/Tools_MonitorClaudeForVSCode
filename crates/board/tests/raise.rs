//! Choosing which window a session belongs to (FR-33, ADR-0009).
//!
//! The part of that chain with no operating system in it: given the editor lock files and a
//! port, which folder is that window showing, and what should be looked for in its title.
//!
//! **These inputs are constructed, not recorded.** There is no `ide/*.lock` fixture — the
//! files carry an auth token, and `docs/test-plan.md` §3 keeps them out of the corpus for
//! exactly that reason. So the shapes below are written from `docs/observation-sources.md`
//! §2.2 rather than cut from a recording, and they are deliberately limited to the two fields
//! this product is allowed to read.

use mcv_board::raise::{
    Lock, Outcome, basename, belongs_to_workspace, editor_cli, editor_of, only_unclaimed_window,
    session_uri, title_matches, title_needle, title_needle_of,
};

fn lock(port: u16, folders: &[&str]) -> Lock {
    Lock {
        port,
        workspace_folders: folders.iter().map(|f| (*f).to_owned()).collect(),
        // Named only where a case is about the editor's identity (TC-108d).
        ide_name: String::new(),
    }
}

#[test]
fn the_needle_is_the_folders_last_segment() {
    let locks = vec![
        lock(45123, &["C:/work/sample-repo"]),
        lock(45124, &["C:/work/other-repo"]),
    ];

    assert_eq!(title_needle(&locks, 45123).as_deref(), Some("sample-repo"));
    assert_eq!(title_needle(&locks, 45124).as_deref(), Some("other-repo"));
    assert_eq!(
        title_needle(&locks, 45999),
        None,
        "a port no lock claims names no window",
    );
}

/// The port is what identifies the window — not the lock's own `pid`, which names the whole
/// VS Code instance. Measured: 82 lock files, 31 of them sharing a pid with another lock, up
/// to five at a time, every one naming a different workspace.
#[test]
fn two_windows_of_one_editor_are_told_apart_by_port() {
    let locks = vec![
        lock(45123, &["C:/work/sample-repo"]),
        lock(45124, &["C:/work/third-repo"]),
    ];

    assert_ne!(
        title_needle(&locks, 45123),
        title_needle(&locks, 45124),
        "two windows of the same editor must not resolve to the same workspace",
    );
}

#[test]
fn a_lock_with_no_folder_names_nothing() {
    assert_eq!(title_needle(&[lock(45123, &[])], 45123), None);
}

#[test]
fn a_basename_is_taken_off_either_separator() {
    assert_eq!(
        basename("C:/work/sample-repo").as_deref(),
        Some("sample-repo")
    );
    assert_eq!(
        basename(r"C:\work\sample-repo").as_deref(),
        Some("sample-repo")
    );
    assert_eq!(
        basename("C:/work/sample-repo/").as_deref(),
        Some("sample-repo")
    );
    assert_eq!(
        basename("/home/dev/my_project").as_deref(),
        Some("my_project")
    );
    assert_eq!(basename(""), None);
}

/// The title match is containment, and deliberately so: `window.title` is a user setting, so
/// a strict parse would break on anyone who customised it. The answer to that is not a
/// cleverer match but the verification step — a raise that finds nothing fails **visibly**
/// (TC-75) instead of quietly doing nothing.
#[test]
fn the_title_match_is_containment_and_says_so() {
    assert!(title_matches(
        "sample-repo - Visual Studio Code",
        "sample-repo"
    ));
    assert!(title_matches(
        "● board.css - sample-repo - Visual Studio Code",
        "sample-repo",
    ));
    assert!(
        !title_matches("other-repo - Visual Studio Code", "sample-repo"),
        "a different workspace is a different window",
    );
    assert!(
        !title_matches("", "sample-repo"),
        "a window with no title carries no evidence",
    );
    assert!(
        !title_matches("anything at all", ""),
        "an empty needle must not match every window on the desktop",
    );
}

/// L-6, stated rather than hidden: two folders whose last segment matches are
/// indistinguishable here, because the folder basename is the only thing joining the
/// extension host to a window at the OS level.
#[test]
fn folders_sharing_a_last_segment_are_not_distinguishable() {
    let needle = title_needle(&[lock(45123, &["C:/work/one/board"])], 45123).expect("a needle");
    assert!(
        title_matches("board - Visual Studio Code", &needle),
        "this is the limitation FR-33 names, and it is not a defect to be patched over",
    );
}

#[test]
fn an_outcome_distinguishes_nothing_found_from_a_refusal() {
    // The distinction the ADR insisted on: `FlashWindowEx` returned success on all three
    // measured runs while raising nothing, so "asked and it did not come" has to be its own
    // answer, separate from "there was nothing to ask".
    assert_ne!(Outcome::NotFound, Outcome::Refused);
    assert_ne!(Outcome::Raised, Outcome::Refused);
}

/// A title match alone is not enough, and this is why the chain asks a second question.
///
/// Run against a live machine, the needle `"Tools_MonitorClaudeForVSCode"` matched the VS
/// Code window **and** a browser window showing a pull request for the repository of the
/// same name. Raising the browser would have been a confident, visible, wrong answer — worse
/// than doing nothing, because the user would believe the board had understood them.
///
/// The second question is process ancestry, not a product name in the title: an edition, a
/// locale, or a customised title bar must not decide whether a click works.
#[test]
fn a_title_match_is_not_by_itself_the_editors_window() {
    let needle = "Tools_MonitorClaudeForVSCode";

    let editor_window =
        "feat/scaffold-workspace - Tools_MonitorClaudeForVSCode - Visual Studio Code - Insiders";
    let browser_window =
        "Pull Request #8 - TMR-Suke3/Tools_MonitorClaudeForVSCode - Microsoft Edge";

    assert!(title_matches(editor_window, needle));
    assert!(
        title_matches(browser_window, needle),
        "this is the point: the browser matches too, and the title cannot tell them apart",
    );
}

// ------------------------------------------------- choosing among the host's ports

/// TC-105 (FR-33) — **the extension host listens on more than one port**, and only one of
/// them is Claude Code's.
///
/// The chain took the first port the process table gave it, on a written assumption that a
/// host listens on one. A live machine disproved it: 54851, 58915 and 59126 for one host,
/// with the lock file under 59126. The first was chosen, no lock claimed it, and every click
/// on that window's sessions did nothing — the failure that reads exactly like a board that
/// is ignoring the mouse (ADR-0027).
#[test]
fn tc_105_the_port_with_a_lock_is_the_one_chosen() {
    let locks = vec![lock(59126, &["C:/work/sample-repo"])];
    // The order a live table gave them, with the answer last.
    let ports = [54851, 58915, 59126];

    assert_eq!(
        title_needle_of(&locks, &ports).as_deref(),
        Some("sample-repo"),
        "a port with no lock must be passed over, not treated as the answer"
    );
}

/// TC-105b — passing over a port is not the same as ignoring what it says. A lock that exists
/// but names no folder is skipped too, because it yields nothing to look for in a title.
#[test]
fn tc_105b_a_port_whose_lock_names_no_folder_is_passed_over() {
    let locks = vec![lock(16147, &[]), lock(59126, &["C:/work/sample-repo"])];

    assert_eq!(
        title_needle_of(&locks, &[16147, 59126]).as_deref(),
        Some("sample-repo")
    );
    // ...and when that is the only lock there is, the chain ends here rather than guessing.
    assert_eq!(title_needle_of(&[lock(16147, &[])], &[16147, 63698]), None);
}

/// TC-105c — no port of the host has a lock. This is an editor window with no folder open,
/// which has no folder in its title either: there is nothing to match on, and the raise ends
/// as `NotFound` rather than as a wrong window.
#[test]
fn tc_105c_a_host_with_no_claimed_port_yields_nothing() {
    let locks = vec![lock(45123, &["C:/work/other-repo"])];

    assert_eq!(title_needle_of(&locks, &[54851, 58915]), None);
    assert_eq!(title_needle_of(&locks, &[]), None);
}

// ------------------------------------------------ the window with no folder open

/// Titles as VS Code actually writes them, including the one that started this: a window with
/// no folder open names the **file** on screen and nothing else, so there is no folder in it
/// to match on. Measured live 2026-08-28.
const WITH_FOLDER: &str = "board.rs - Tools_MonitorClaudeForVSCode - Visual Studio Code - Insiders";
const NO_FOLDER: &str = "メモを整理して - Visual Studio Code - Insiders";

fn window(id: isize, title: &str) -> (isize, String) {
    (id, title.to_owned())
}

/// TC-107 (FR-33) — the window no other session accounts for is this one.
///
/// An extension host is per window, so the instance's other hosts name all but one of its
/// windows. Striking those out leaves the window that has no folder to be recognised by —
/// identified not by recognising it, but because nothing else can be it (ADR-0028).
#[test]
fn tc_107_the_last_unclaimed_window_is_the_answer() {
    let windows = [window(1, WITH_FOLDER), window(2, NO_FOLDER)];
    let claimed = ["Tools_MonitorClaudeForVSCode".to_owned()];

    assert_eq!(only_unclaimed_window(&windows, &claimed), Some(2));
}

/// TC-107b — **exactly one, or nothing.** Two windows left means two sessions could own
/// either, and this product does not raise a window it cannot name: a wrong window is worse
/// than a visible failure (TC-75). The same rule L-1 states for two windows sharing a folder.
#[test]
fn tc_107b_two_unnamed_windows_raise_neither() {
    let windows = [
        window(1, WITH_FOLDER),
        window(2, NO_FOLDER),
        window(3, "notes.md - Visual Studio Code - Insiders"),
    ];
    let claimed = ["Tools_MonitorClaudeForVSCode".to_owned()];

    assert_eq!(
        only_unclaimed_window(&windows, &claimed),
        None,
        "with two windows unaccounted for, neither may be raised"
    );
}

/// TC-107c — every window accounted for leaves none, which is also nothing to raise. This is
/// the shape of a stale process table rather than of a real window, and it must not fall
/// through to "raise the first one".
#[test]
fn tc_107c_no_window_left_is_not_an_answer() {
    let windows = [window(1, WITH_FOLDER)];
    let claimed = ["Tools_MonitorClaudeForVSCode".to_owned()];

    assert_eq!(only_unclaimed_window(&windows, &claimed), None);
    assert_eq!(only_unclaimed_window(&[], &claimed), None);
}

/// TC-107d — one window and no siblings to strike out is the single-window editor, and it is
/// the answer. Nothing else can be it, which is the whole of the rule.
#[test]
fn tc_107d_a_lone_window_needs_no_elimination() {
    assert_eq!(only_unclaimed_window(&[window(7, NO_FOLDER)], &[]), Some(7));
}

// --------------------------------------------- opening the session's own tab

/// A lock as the editor writes it, with the editor's own name in it.
fn named_lock(port: u16, ide: &str) -> Lock {
    let mut lock = lock(port, &["C:/work/sample-repo"]);
    lock.ide_name = ide.to_owned();
    lock
}

/// TC-108 (FR-33) — each editor is sent to through its **own** URL scheme.
///
/// The measured route: the extension registers a URI handler that `package.json` does not
/// declare, and `…/open?session=<id>` reveals that session and focuses its message box
/// (ADR-0029). The scheme is the part that says *which editor* — sending a `vscode://` URL
/// from an Insiders window would hand it to a different application.
#[test]
fn tc_108_each_editor_has_its_own_scheme() {
    let session = "2fb810f6-104e-4c12-ab30-abdaaca1366d";

    assert_eq!(
        session_uri("Visual Studio Code - Insiders", session).as_deref(),
        Some(
            "vscode-insiders://Anthropic.claude-code/open?session=2fb810f6-104e-4c12-ab30-abdaaca1366d"
        )
    );
    assert_eq!(
        session_uri("Visual Studio Code", session).as_deref(),
        Some("vscode://Anthropic.claude-code/open?session=2fb810f6-104e-4c12-ab30-abdaaca1366d")
    );
}

/// TC-108b — **an editor this build does not know is not guessed at.** Defaulting to
/// `vscode://` for a fork would hand the URL to whichever editor the machine associates with
/// that scheme, which is a different application from the one the user clicked on.
#[test]
fn tc_108b_an_unknown_editor_is_not_guessed_at() {
    let session = "2fb810f6-104e-4c12-ab30-abdaaca1366d";
    for ide in ["", "Cursor", "Windsurf", "Visual Studio", "code"] {
        assert_eq!(
            session_uri(ide, session),
            None,
            "{ide:?} is not an editor this build can address"
        );
    }
}

/// TC-108c — the session id goes into a URL handed to another program, so it is checked
/// rather than trusted. It comes from a file name and a parsed JSON field, so it is a session
/// id or it is nothing — and "or it is nothing" is the half worth asserting.
#[test]
fn tc_108c_only_a_session_id_shaped_string_is_sent() {
    for bad in [
        "",
        "abc def",
        "abc&prompt=rm+-rf",
        "abc#frag",
        "abc/../..",
        "abc%20",
        "abc?x=1",
    ] {
        assert_eq!(
            session_uri("Visual Studio Code", bad),
            None,
            "{bad:?} must not be pasted into a URL"
        );
    }
    assert!(session_uri("Visual Studio Code", "0a1b-2c3d-EF").is_some());
}

/// The same guard, asserted one character at a time.
///
/// Every bad input above fails for more than one reason — `"abc&prompt=rm+-rf"` carries `&`,
/// `=` and `+` — so TC-108c stays green even if the guard stops rejecting any single one of
/// them. A mutation audit measured exactly that: admitting `&`, the one character that ends a
/// URL parameter and begins another, left TC-108c passing.
///
/// So each case below differs from a valid id by one character and nothing else, which is
/// what makes the failure attributable to that character.
#[test]
fn no_single_character_that_changes_a_urls_meaning_is_admitted() {
    for bad in [
        "a&b", "a=b", "a#b", "a?b", "a/b", "a%b", "a b", "a+b", "a.b", "a:b", "a@b",
    ] {
        assert_eq!(
            session_uri("Visual Studio Code", bad),
            None,
            "{bad:?} is a valid id but for one character, and that character must be refused"
        );
    }
    // The control: the same shape with the one separator an id is allowed to carry.
    assert!(session_uri("Visual Studio Code", "a-b").is_some());
}

/// TC-108d — the editor's name comes from the lock that claimed the port, walked the same way
/// the folder is (TC-105): the host listens on several ports and only one is Claude Code's.
#[test]
fn tc_108d_the_editors_name_comes_from_the_lock_that_claimed_the_port() {
    let locks = vec![named_lock(59126, "Visual Studio Code - Insiders")];

    assert_eq!(
        editor_of(&locks, &[54851, 58915, 59126]).as_deref(),
        Some("Visual Studio Code - Insiders")
    );
    assert_eq!(editor_of(&locks, &[54851]), None);
    assert_eq!(editor_of(&[], &[59126]), None);
    // A lock that names no editor is not an answer either: there would be no scheme to use.
    assert_eq!(editor_of(&[lock(59126, &["C:/work/x"])], &[59126]), None);
}

/// TC-109 (FR-33) — **the documented precondition.** The editor resumes a session only where
/// it belongs to the window's workspace; where it does not, it starts a *fresh* conversation
/// instead, so sending anyway opens a second copy in the wrong window. That is the failure
/// that was seen before this was checked (ADR-0029).
#[test]
fn tc_109_a_session_outside_the_windows_workspace_is_not_sent() {
    let repo = vec![r"c:\Users\me\Documents\GitHub\board".to_owned()];

    assert!(belongs_to_workspace(
        r"c:\Users\me\Documents\GitHub\board",
        &repo
    ));
    // A folder inside the workspace still belongs to it.
    assert!(belongs_to_workspace(
        r"c:\Users\me\Documents\GitHub\board\crates\core",
        &repo
    ));
    // A different folder does not, and neither does a prefix that is not a path boundary.
    assert!(!belongs_to_workspace(r"C:\Users\me", &repo));
    assert!(!belongs_to_workspace(
        r"c:\Users\me\Documents\GitHub\board-notes",
        &repo
    ));
}

/// TC-109b — the two strings come from different files written by different programs, and the
/// same folder has been seen as `c:\…` in one and `C:/…` in the other.
#[test]
fn tc_109b_the_comparison_survives_case_and_separators() {
    let folders = vec!["C:/Users/me/Documents/GitHub/board".to_owned()];
    assert!(belongs_to_workspace(
        r"c:\Users\me\Documents\GitHub\board",
        &folders
    ));
    assert!(belongs_to_workspace(
        r"C:\USERS\ME\DOCUMENTS\GITHUB\BOARD\crates",
        &folders
    ));
}

/// TC-109c — **a window with no folder open has no workspace to belong to**, so nothing is
/// ever sent for it. This is the folderless case: the editor could not resume into that window
/// either, so declining is the whole of the correct behaviour.
#[test]
fn tc_109c_a_window_with_no_folder_never_matches() {
    assert!(!belongs_to_workspace(r"C:\Users\me", &[]));
    assert!(!belongs_to_workspace(r"C:\Users\me", &[String::new()]));
}

/// TC-110 (FR-33) — the editor's own launcher is found beside its executable, and **only when
/// there is exactly one candidate**.
///
/// The shell's URL association runs `Code.exe --open-url`, which that executable rejects, so
/// every URL sent that way was discarded in silence; `bin/*.cmd` is the wrapper that works.
/// A real install also held `new_code-insiders.cmd` — a staged update — and choosing between
/// them by name would be guesswork about another program's upgrade mechanics.
#[test]
fn tc_110_the_launcher_is_found_beside_the_executable() {
    let dir = std::env::temp_dir().join(format!("mcv-cli-{}", std::process::id()));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("a temp install");
    let exe = dir.join("Code - Insiders.exe");
    std::fs::write(&exe, b"").expect("an executable");
    std::fs::write(bin.join("code-insiders.cmd"), b"").expect("a launcher");
    // Not a launcher: the extensionless shell script that ships beside it.
    std::fs::write(bin.join("code-insiders"), b"").expect("a shell script");

    assert_eq!(
        editor_cli(&exe).as_deref(),
        Some(bin.join("code-insiders.cmd").as_path())
    );

    // A staged update is skipped rather than chosen between.
    std::fs::write(bin.join("new_code-insiders.cmd"), b"").expect("a staged update");
    assert_eq!(
        editor_cli(&exe).as_deref(),
        Some(bin.join("code-insiders.cmd").as_path())
    );

    // Genuinely ambiguous: decline.
    std::fs::write(bin.join("codium.cmd"), b"").expect("a second launcher");
    assert_eq!(editor_cli(&exe), None);

    let _ = std::fs::remove_dir_all(&dir);
}

/// TC-110b — no `bin` at all is not an error, it is an editor this build cannot launch.
#[test]
fn tc_110b_an_install_with_no_launcher_is_declined() {
    let dir = std::env::temp_dir().join(format!("mcv-nocli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp install");
    let exe = dir.join("Code.exe");
    std::fs::write(&exe, b"").expect("an executable");

    assert_eq!(editor_cli(&exe), None);
    let _ = std::fs::remove_dir_all(&dir);
}
