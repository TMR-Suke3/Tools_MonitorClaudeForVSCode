//! Deciding which window a session belongs to (FR-33).
//!
//! The chain is [ADR-0009](../../../docs/adr/0009-map-a-session-to-its-window.md)'s, and it
//! was measured rather than reasoned:
//!
//! ```text
//! sessions/<pid>.json  ->  claude.exe
//!    parent            ->  the extension host — one per WINDOW
//!    listening port    ->  ide/<port>.lock  ->  that window's workspaceFolders
//!    title match       ->  HWND
//! ```
//!
//! Two facts make the middle of that chain necessary. A lock file's own `pid` field names
//! the VS Code *instance*, not the window — measured across 82 lock files, 31 of which
//! shared a pid with another lock, up to five at a time. And the extension host is per
//! window, so the process that owns the listening port **is** the window.
//!
//! This module holds the part of that with no operating system in it: given the lock files
//! and a port, which folder is this window showing, and what should be looked for in a
//! window title. The Win32 half lives in [`crate::win`].

use std::path::Path;

use serde::Deserialize;

/// One `~/.claude/ide/<port>.lock`.
///
/// The `authToken` in these files is **never read** — it is not named here, so serde steps
/// over it (`docs/observation-sources.md` §1, rule 2). Neither is `messagingSocketPath`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Lock {
    /// The port the file is named after, filled in from the file name rather than the body.
    #[serde(skip)]
    pub port: u16,
    /// The folders that window has open.
    #[serde(rename = "workspaceFolders", default)]
    pub workspace_folders: Vec<String>,
    /// Which editor this is — `"Visual Studio Code"`, `"Visual Studio Code - Insiders"`, and
    /// whatever else registers the extension.
    ///
    /// Read for one reason: each editor owns a different URL scheme, and revealing a session
    /// means handing a URL to *that* editor rather than to whichever one the machine has
    /// associated with `vscode://` ([`session_uri`]). It is a product name, not a path and
    /// not a token.
    #[serde(rename = "ideName", default)]
    pub ide_name: String,
}

impl Lock {
    /// Reads one lock file, taking its port from the file name.
    ///
    /// Returns `None` for anything that is not a readable `<port>.lock` — stale files
    /// accumulate here and are never cleaned up (measured: 3 live of 84), so an unreadable
    /// one is the normal case rather than a fault (FR-40).
    #[must_use]
    pub fn read(path: &Path) -> Option<Self> {
        let port: u16 = path.file_stem()?.to_str()?.parse().ok()?;
        let text = std::fs::read_to_string(path).ok()?;
        let mut lock: Self = serde_json::from_str(&text).ok()?;
        lock.port = port;
        Some(lock)
    }
}

/// What to look for in a window title, for the window listening on `port`.
///
/// VS Code's default title carries the workspace folder's **basename**, which is the only
/// thing at the OS level that joins the extension host to the window a user sees: the host
/// owns no window, and the window belongs to a sibling renderer process. That is the whole
/// of limitation L-6 — two folders whose last path segment matches are indistinguishable
/// here — and it is stated in FR-33 rather than treated as a defect.
///
/// Returns `None` when no lock claims that port, or when the lock names no folder.
#[must_use]
pub fn title_needle(locks: &[Lock], port: u16) -> Option<String> {
    let lock = locks.iter().find(|lock| lock.port == port)?;
    let folder = lock.workspace_folders.first()?;
    basename(folder)
}

/// The same, over every port one process is listening on.
///
/// **The extension host listens on more than one**, and only one of them is Claude Code's.
/// Which one is not decidable from the process table — the ports carry no label — so this
/// does not choose: it asks each in turn and takes the first that a lock file claims *and*
/// that names a folder. The lock files are the authority, because a lock is written by the
/// thing this is looking for
/// ([ADR-0027](../../../docs/adr/0027-pick-the-editors-port-by-its-lock-file.md)).
///
/// Order is the table's, which is arbitrary; it does not matter, because at most one of a
/// host's ports has a lock. If that ever stops being true this takes the first, and the
/// wrong answer is a window of the same editor — not somebody else's window.
#[must_use]
pub fn title_needle_of(locks: &[Lock], ports: &[u16]) -> Option<String> {
    ports.iter().find_map(|port| title_needle(locks, *port))
}

/// The last segment of a path, whichever separator it uses.
///
/// Taken from the folder the lock file states, never from the encoded directory name under
/// `projects/` — that encoding is lossy and cannot be decoded back
/// (`docs/observation-sources.md` §2.3).
#[must_use]
pub fn basename(folder: &str) -> Option<String> {
    folder
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .map(str::to_owned)
}

/// Whether a window title belongs to the workspace named by `needle`.
///
/// Deliberately a containment test rather than a parse. VS Code's title is a user setting —
/// `window.title` can be anything — so this cannot be strict, and the ADR's answer to that
/// is not a cleverer match but the verification step: when the title does not carry the
/// folder, the raise fails **visibly** (TC-75) instead of quietly doing nothing.
#[must_use]
pub fn title_matches(title: &str, needle: &str) -> bool {
    !needle.is_empty() && title.contains(needle)
}

/// The window of an editor instance that no other session's folder accounts for.
///
/// **The answer for a window with no folder open.** Its `workspaceFolders` is empty and its
/// title carries no folder either — one measured live read
/// `"メモを整理して - Visual Studio Code - Insiders"`, which names the file on screen and
/// nothing else — so there is no string to match on and the chain of
/// [`title_needle`] has nothing to work with.
///
/// What is left is arithmetic. An extension host is per window, so *n* hosts on one instance
/// means *n* windows. Take the instance's windows, strike out every one whose title carries
/// the folder of some **other** host, and if exactly one remains it is this host's — not by
/// recognising it, but because nothing else can be it
/// ([ADR-0028](../../../docs/adr/0028-name-the-last-window-by-elimination.md)).
///
/// **Exactly one, or nothing.** Two windows left means two sessions could own either, and
/// this product does not raise a window it cannot name: a wrong window is worse than a
/// visible failure (TC-75). That is the same rule limitation L-1 states for two windows
/// sharing a folder, applied to the case where the folder is absent instead of duplicated.
#[must_use]
pub fn only_unclaimed_window(windows: &[(isize, String)], claimed_by: &[String]) -> Option<isize> {
    let mut unclaimed = windows
        .iter()
        .filter(|(_, title)| !claimed_by.iter().any(|needle| title_matches(title, needle)));
    let first = unclaimed.next()?;
    unclaimed.next().is_none().then_some(first.0)
}

/// The lock that claimed one of these ports, if any did.
///
/// One lookup for the two questions that follow it — which editor this is, and which folders
/// its window has open — so they cannot end up answered from different locks.
#[must_use]
pub fn lock_for<'a>(locks: &'a [Lock], ports: &[u16]) -> Option<&'a Lock> {
    ports
        .iter()
        .find_map(|port| locks.iter().find(|lock| lock.port == *port))
}

/// Whether a session's working directory belongs to a window's workspace.
///
/// **The documented precondition**, and the one this product was missing: the editor's URL
/// handler resumes a session only if *"the session must belong to the workspace currently
/// open in VS Code"*, and if it does not, *"a fresh conversation starts instead"*. So a URL
/// sent to the wrong window is not a no-op — it opens a second copy of that session there.
/// Asking first is what turns that from a hazard into a case the board simply declines
/// ([ADR-0029](../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)).
///
/// A window with **no folder open** has no workspace for anything to belong to, so nothing is
/// ever sent for it. That is not a gap: such a window is the one the editor cannot resume
/// into either.
///
/// Compared case-insensitively and across both separators, because these two strings come
/// from different files written by different programs: `sessions/<pid>.json` gives `cwd` and
/// the lock gives `workspaceFolders`, and the same folder has been seen as `c:\…` in one and
/// `C:/…` in the other.
#[must_use]
pub fn belongs_to_workspace(cwd: &str, folders: &[String]) -> bool {
    let tidy = |path: &str| {
        path.replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    let cwd = tidy(cwd);
    folders.iter().any(|folder| {
        let folder = tidy(folder);
        !folder.is_empty() && (cwd == folder || cwd.starts_with(&format!("{folder}/")))
    })
}

/// The editor's own command-line launcher, beside its executable.
///
/// **Not the shell's URL association**, which is what this used and why nothing happened. On
/// the machine that found this, `HKCR\vscode-insiders` runs `Code - Insiders.exe --open-url`,
/// and that executable answers `bad option: --open-url`: only the `bin/*.cmd` wrapper works,
/// because it sets `ELECTRON_RUN_AS_NODE` and hands the arguments to the editor's `cli.js`.
/// Every URL the board "sent" through the association was discarded in silence.
///
/// Going through the launcher beside *the running editor's own executable* also aims better
/// than the association ever could: the URL reaches the installation whose window was just
/// raised, rather than whichever copy owns the scheme.
///
/// **Exactly one candidate, or none.** `bin/` also held `new_code-insiders.cmd` — a staged
/// update — and picking between them by name would be guesswork about another program's
/// upgrade mechanics. Those are skipped, and anything still ambiguous is declined.
#[must_use]
pub fn editor_cli(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let bin = exe.parent()?.join("bin");
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(bin)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"))
        })
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("new_"))
        })
        .collect();
    found.sort();
    (found.len() == 1).then(|| found.remove(0))
}

/// The editor's own name, from the lock that claimed one of these ports.
///
/// Needed only on the opt-in path that reveals a session, which is why the locks are read
/// again there rather than carried out of [`raise_window_of`]: on the ordinary path nothing
/// asks this question, and reading 84 small files for an answer nobody wants is a cost the
/// common case should not pay.
#[must_use]
pub fn editor_of(locks: &[Lock], ports: &[u16]) -> Option<String> {
    let lock = lock_for(locks, ports)?;
    (!lock.ide_name.is_empty()).then(|| lock.ide_name.clone())
}

/// The URL that brings one session's own tab to the front, for an editor that has one.
///
/// **An undocumented surface, entered deliberately.** The extension registers a URI handler
/// that `package.json` does not declare, and `…/open?session=<id>` reveals that session's
/// panel and puts the cursor in its input — measured on a live machine, which is the only
/// reason it is here at all
/// ([ADR-0029](../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)).
/// It will break without notice, and everything around it is built to fail quietly when it
/// does.
///
/// **`None` for an editor this build does not know**, which is the point of the match rather
/// than a gap in it: every editor owns its own scheme, and guessing `vscode://` for a fork
/// would hand the URL to whichever editor the machine associates with it — a different
/// application from the one the user clicked.
#[must_use]
pub fn session_uri(ide_name: &str, session: &str) -> Option<String> {
    let scheme = match ide_name {
        "Visual Studio Code" => "vscode",
        "Visual Studio Code - Insiders" => "vscode-insiders",
        "Visual Studio Code - Exploration" => "vscode-exploration",
        "VSCodium" => "vscodium",
        _ => return None,
    };
    // The id comes from a file name and a JSON field this build has already parsed, so it is
    // a session id or it is nothing. Checked rather than trusted anyway: this string is about
    // to become part of a URL handed to another program, and the one character that would
    // change its meaning is the one that ends the parameter.
    if session.is_empty()
        || !session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    Some(format!(
        "{scheme}://Anthropic.claude-code/open?session={session}"
    ))
}

/// How a raise ended, so the board can say (FR-33, TC-75, TC-76).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The window came forward, confirmed by reading the foreground window back.
    Raised,
    /// Nothing was found to raise — no port, no lock, or no window whose title matched.
    NotFound,
    /// A window was found and asked to come forward, and did not.
    ///
    /// This is the case a return value cannot detect: `FlashWindowEx` returned success on
    /// all three measured runs while raising nothing. Comparing the foreground window
    /// afterwards is what makes it visible.
    Refused,
}

/// What a raise did, and to what.
///
/// The window comes back with the outcome because of the opt-in that follows it: revealing a
/// session means sending a URL while the raised window is still the active one, and that has
/// to be checked against the window the raise actually chose rather than one looked up again
/// afterwards (ADR-0029).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Raise {
    pub outcome: Outcome,
    /// The window that was found and asked to come forward, if any was.
    pub window: Option<isize>,
}

/// Every readable lock file in `~/.claude/ide/`.
///
/// Unreadable and stale ones are skipped without comment: the directory is never cleaned up
/// — measured at 3 live of 84 — so most of what is here describes windows that closed long
/// ago, and that is the normal state rather than a fault (FR-40).
#[must_use]
pub fn read_locks(ide_dir: &Path) -> Vec<Lock> {
    let Ok(entries) = std::fs::read_dir(ide_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| Lock::read(&entry.ok()?.path()))
        .collect()
}

/// Runs the whole chain for a session's process: find its window, and bring it forward.
///
/// `belongs_to_editor` decides whether a window that matched by title really is the editor's.
/// It has to be asked, because **a title match alone is not enough**: run against a live
/// machine, the needle `"Tools_MonitorClaudeForVSCode"` matched the VS Code window *and* a
/// browser window showing a pull request for the repository of the same name. Raising that
/// browser would have been a confident, visible, wrong answer — worse than doing nothing.
///
/// The caller resolves it by process ancestry rather than by matching a product name in the
/// title: the window's owning process must descend from the same VS Code instance the
/// extension host does. That is exact, and it does not assume an edition, a locale, or a
/// user who has not customised their title bar.
///
/// Every step can fail for an ordinary reason — the editor was closed, the title was
/// customised, the process died between the click and the call — and each ends as
/// [`Outcome::NotFound`] or [`Outcome::Refused`], never as a silent nothing (TC-75).
#[cfg(windows)]
#[must_use]
pub fn raise_window_of(
    parent_pid: Option<u32>,
    ide_dir: &Path,
    sibling_hosts: &[u32],
    belongs_to_editor: &mut dyn FnMut(u32) -> bool,
) -> Raise {
    use crate::win;

    let nothing = Raise {
        outcome: Outcome::NotFound,
        window: None,
    };

    // The extension host is the parent of `claude.exe`, and it is per window.
    let Some(host) = parent_pid else {
        return nothing;
    };
    let ports = win::listening_ports_of(host);
    if ports.is_empty() {
        return nothing;
    }
    let locks = read_locks(ide_dir);

    // This instance's windows. The host owns no window of its own; the windows belong to the
    // editor, and asking who owns them is what keeps a browser window whose title happens to
    // name the workspace out of the answer.
    let windows: Vec<(isize, String)> = win::visible_windows()
        .into_iter()
        .filter(|(window, _)| belongs_to_editor(win::owner_of(*window)))
        .collect();

    let window = match title_needle_of(&locks, &ports) {
        // The ordinary case: this window has a folder open, and its name is in the title.
        Some(needle) => windows
            .into_iter()
            .find(|(_, title)| title_matches(title, &needle))
            .map(|(window, _)| window),
        // No folder open, so nothing to recognise. Name it by what it cannot be instead.
        None => {
            let claimed: Vec<String> = sibling_hosts
                .iter()
                .filter_map(|host| title_needle_of(&locks, &win::listening_ports_of(*host)))
                .collect();
            only_unclaimed_window(&windows, &claimed)
        }
    };
    let Some(window) = window else {
        return nothing;
    };

    Raise {
        outcome: if win::raise(window) {
            Outcome::Raised
        } else {
            Outcome::Refused
        },
        window: Some(window),
    }
}
