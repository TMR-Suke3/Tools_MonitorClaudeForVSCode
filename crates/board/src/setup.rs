//! The hook setup window: the commands behind `ui/setup.html`.
//!
//! The flow is `docs/hook-setup.md` §4, and the shape of this file follows one rule from it:
//! **the front end never decides anything that matters**. What the change is, whether the
//! file can be read, where the backup went, whether an event has actually arrived — all
//! answered here, from [`crate::hooks`] and [`crate::events`], and the window paints the
//! answers.
//!
//! The window is created on demand rather than declared in `tauri.conf.json`, because it is
//! opened rarely and should not exist the rest of the time.
//!
//! It holds four screens, not one. The flow above installs the hooks; the **diagnosis**
//! ([`hook_diagnosis`], `docs/hook-setup.md` §5) says which of hook mode's four preconditions
//! is broken and what single action repairs it — FR-55's second half; the **reveal**
//! explanation is the opt-in behind opening a clicked session's own tab (ADR-0029); and
//! **shortcuts** is where the two global keys are changed or cleared (FR-61).
//!
//! They share this window deliberately. The first two are the same argument as
//! [ADR-0026](../../../docs/adr/0026-the-diagnosis-lives-in-the-setup-window.md) — every
//! question about hook mode has one place to be asked, and the same answers — and the other
//! two joined them because a settings window the user has to find twice is two windows to
//! find.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::app::App;
use crate::hooks::{self, Installed};
use crate::watch::live::{claude_dir, now_millis};

/// The window's label, and the handle everything below finds it by.
const LABEL: &str = "hook-setup";

/// Which of the window's two screens to open on.
///
/// Exists for the screenshot scripts. Both screens are reached by a menu item in real use,
/// which a capture script cannot click — and a screen that cannot be photographed is a screen
/// that stops being reviewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The offer, and the install flow behind it (§4).
    Offer,
    /// The diagnosis (§5, FR-55).
    Diagnosis,
    /// The explanation behind opening the clicked session's own tab (ADR-0029).
    Reveal,
    /// Changing the two global shortcuts (FR-61).
    Shortcuts,
}

impl Screen {
    /// What the window is called while it is showing this. Each screen asks a different
    /// question, and a window titled after the wrong one is a window the user has to read
    /// twice to place.
    const fn title(self) -> &'static str {
        match self {
            Self::Offer => "Notice waiting instantly",
            Self::Diagnosis => "Is anything reporting in?",
            Self::Reveal => "Open the session's tab too",
            Self::Shortcuts => "Change the keys",
        }
    }

    const fn url(self) -> &'static str {
        match self {
            Self::Offer => "setup.html",
            Self::Diagnosis => "setup.html?step=diagnose",
            Self::Reveal => "setup.html?step=reveal",
            Self::Shortcuts => "setup.html?step=shortcuts",
        }
    }

    const fn step(self) -> &'static str {
        match self {
            Self::Offer => "explain",
            Self::Diagnosis => "diagnose",
            Self::Reveal => "reveal",
            Self::Shortcuts => "shortcuts",
        }
    }
}

/// Where the helper is: beside the board's own executable.
///
/// Not searched for on `PATH` and not remembered from a previous install — the entry written
/// into the user's settings has to name a program that is still there, and the only one this
/// build can vouch for is the one shipped next to it.
#[must_use]
pub fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "mcv-hook.exe"
    } else {
        "mcv-hook"
    };
    Some(exe.with_file_name(name))
}

/// The file the entries go in.
#[must_use]
pub fn settings_path() -> Option<PathBuf> {
    Some(claude_dir()?.join("settings.json"))
}

/// What the window needs to draw itself before the user has done anything.
#[derive(Debug, Clone, Serialize)]
pub struct SetupState {
    pub theme: &'static str,
    pub chrome: crate::view::Chrome,
    /// `"no"`, `"current"`, or `"other"` — which step to open on (FR-54).
    pub installed: &'static str,
    /// False when the helper is not where it should be, which is the one problem setup
    /// cannot write its way out of.
    pub helper_found: bool,
    pub settings_path: String,
    /// `"exact"`, `"went-quiet"` or `"inferred"` — what the board is earning right now
    /// (FR-55).
    ///
    /// Answered here rather than in the window for the same reason as everything else on
    /// this screen: it is a fact about the machine, and the front end paints answers.
    pub mode: &'static str,
}

/// The change, as the preview step shows it (FR-43).
#[derive(Debug, Clone, Serialize)]
pub struct PreviewedChange {
    pub path: String,
    pub diff: String,
    pub reformats: bool,
    pub nothing_to_do: bool,
    pub added: usize,
    pub removed: usize,
    pub entries_added: usize,
    pub entries_removed: usize,
}

/// What a completed write reports back (FR-44, FR-50).
#[derive(Debug, Clone, Serialize)]
pub struct WriteResult {
    /// Where the backup went, or empty when there was no settings file to back up.
    pub backup: String,
    /// When the write happened. The proof step asks for events *after* this, so that a log
    /// left over from a previous install cannot pass for one.
    pub installed_at: i64,
}

/// What the settings file turned out to be, as the diagnosis has to distinguish it.
///
/// **Three outcomes, not two.** A file that is not there yet and a file that cannot be read
/// look the same to `read_to_string().ok()`, and they call for opposite things: the first is
/// an ordinary first install, and the second is the one case where this product writes
/// nothing and explains (FR-53). Collapsing them is how a diagnosis ends up telling someone
/// to run a setup that is going to refuse.
pub enum SettingsFile {
    /// No file yet. Not a fault: a first install writes the file Claude Code would have.
    Missing,
    /// There, and unreadable — malformed JSON, or a shape this build does not recognise.
    Unreadable(String),
    /// There, and understood. Carries what our own entries look like inside it.
    Read(Installed),
}

/// Reads the settings file once, for whoever is asking.
///
/// Shared by [`hook_state`] and [`hook_diagnosis`] so the two cannot disagree about what is
/// installed — a diagnosis that contradicted the line above it would be worse than none.
fn read_settings() -> SettingsFile {
    let (Some(path), Some(helper)) = (settings_path(), helper_path()) else {
        return SettingsFile::Missing;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return SettingsFile::Missing,
        Err(e) => return SettingsFile::Unreadable(e.to_string()),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => return SettingsFile::Unreadable(e.to_string()),
    };
    match hooks::installed_in(&value, &helper) {
        Ok(installed) => SettingsFile::Read(installed),
        Err(refusal) => SettingsFile::Unreadable(refusal.to_string()),
    }
}

impl SettingsFile {
    /// What is installed, as the rest of the window asks about it. An unreadable file has
    /// nothing of ours in it as far as anything that would *write* is concerned.
    #[must_use]
    pub const fn installed(&self) -> Installed {
        match self {
            Self::Read(installed) => *installed,
            _ => Installed::No,
        }
    }
}

#[tauri::command]
pub fn hook_state(state: tauri::State<'_, App>) -> SetupState {
    let helper = helper_path();
    let settings = settings_path();
    let installed = read_settings().installed();

    SetupState {
        theme: match state.theme {
            mcv_core::palette::Theme::Dark => "dark",
            mcv_core::palette::Theme::Light => "light",
        },
        chrome: chrome_of(&state),
        installed: match installed {
            Installed::No => "no",
            Installed::Current => "current",
            Installed::Other => "other",
        },
        helper_found: helper.is_some_and(|path| path.is_file()),
        mode: crate::events::mode(
            installed != Installed::No,
            crate::events::events_path().and_then(|path| crate::events::latest_since(&path, 0)),
            now_millis(),
        )
        .label(),
        settings_path: settings
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

/// The board's own four colours, so the two windows are recognisably one product.
fn chrome_of(state: &tauri::State<'_, App>) -> crate::view::Chrome {
    state.board.lock().map_or_else(
        |_| fallback_chrome(state.theme),
        |board| board.chrome.clone(),
    )
}

fn fallback_chrome(theme: mcv_core::palette::Theme) -> crate::view::Chrome {
    crate::view::Chrome::of(theme)
}

/// What installing (or removing) would do, without doing it.
///
/// # Errors
///
/// The refusal, in the user's words: nothing is written and the window says why (FR-53).
#[tauri::command]
pub fn hook_preview(install: bool) -> Result<PreviewedChange, String> {
    let path =
        settings_path().ok_or("this computer does not say where Claude Code's settings live")?;
    let helper =
        helper_path().ok_or("the helper program that came with this app could not be located")?;
    if install && !helper.is_file() {
        return Err(format!(
            "the helper program is missing from {}, so there is nothing to install",
            helper.display()
        ));
    }

    // A settings file that is not there yet is an empty one: a first install writes the file
    // Claude Code would have written itself. A file that exists and cannot be *read* is a
    // different thing, and previewing a change to it would promise something the write
    // could not deliver — `hooks::write` applies the same rule.
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_owned(),
        Err(e) => return Err(format!("{} could not be read ({e})", path.display())),
    };
    let change = hooks::preview(&text, install.then_some(helper.as_path()))
        .map_err(|refusal| refusal.to_string())?;

    Ok(PreviewedChange {
        path: path.display().to_string(),
        diff: change.diff,
        reformats: change.reformats,
        nothing_to_do: change.nothing_to_do,
        added: change.added,
        removed: change.removed,
        entries_added: change.entries_added,
        entries_removed: change.entries_removed,
    })
}

/// Writes the change, backup first (FR-44).
///
/// # Errors
///
/// The refusal or the I/O failure, in the user's words. Either way the settings file is the
/// one it was before the call.
#[tauri::command]
pub fn hook_install(install: bool) -> Result<WriteResult, String> {
    let path =
        settings_path().ok_or("this computer does not say where Claude Code's settings live")?;
    let helper =
        helper_path().ok_or("the helper program that came with this app could not be located")?;

    let now = now_millis();
    let written = hooks::write(&path, install.then_some(helper.as_path()), now)
        .map_err(|failure| failure.to_string())?;

    Ok(WriteResult {
        backup: written.backup.display().to_string(),
        installed_at: now,
    })
}

/// One precondition of hook mode, and the single thing that repairs it.
///
/// The shape is the requirement. FR-55 asks for "each precondition and the single action that
/// repairs it", and the reason it is one action rather than an explanation is that the user is
/// not required to know what a hook is. A row that said *why* the entries are missing would be
/// teaching; a row with one button is a repair.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// A stable key the front end matches on, so its markup never depends on the prose.
    pub id: &'static str,
    /// The precondition, in the user's terms rather than the machine's.
    pub what: &'static str,
    /// Whether it holds right now.
    pub ok: bool,
    /// What is true, in one line. The path, the count, the time since — the specific thing,
    /// because "something is wrong" is what the user already knew.
    pub detail: String,
    /// What to do about it. `None` when the row holds: a repair offered for something that is
    /// not broken is how a diagnosis stops being read.
    pub action: Option<Action>,
}

/// The one action a broken row offers.
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    /// What the front end draws and does. One of `install`, `open-settings`, `watch`, or
    /// `tell` — which is the case where the product **cannot** act, and says so rather than
    /// offering a button that would do nothing.
    pub kind: &'static str,
    pub label: &'static str,
}

/// The whole panel (`docs/hook-setup.md` §5, FR-55).
#[derive(Debug, Clone, Serialize)]
pub struct Diagnosis {
    /// The mode, so the panel opens with the conclusion its rows go on to explain.
    pub mode: &'static str,
    pub checks: Vec<Check>,
}

/// Everything the diagnosis is a function of.
///
/// Gathered by [`hook_diagnosis`] and decided by [`diagnose`], which is what makes the panel
/// testable: none of the four situations it exists to describe can be produced on the machine
/// running the tests without breaking that machine's own hook setup.
pub struct Facts<'a> {
    /// Where the helper should be, and whether it is there.
    pub helper: Option<&'a std::path::Path>,
    pub helper_found: bool,
    /// The settings file's path, which the rows quote whether or not it exists.
    pub settings_path: &'a str,
    pub settings: &'a SettingsFile,
    /// When the last event arrived, if any ever has.
    pub last_event: Option<i64>,
    pub now: i64,
}

/// Answers "is this thing on?", one precondition at a time.
#[tauri::command]
#[must_use]
pub fn hook_diagnosis() -> Diagnosis {
    let helper = helper_path();
    let settings = read_settings();
    let path = settings_path().map_or_else(String::new, |p| p.display().to_string());
    diagnose(&Facts {
        helper: helper.as_deref(),
        helper_found: helper.as_ref().is_some_and(|path| path.is_file()),
        settings_path: &path,
        settings: &settings,
        last_event: crate::events::events_path()
            .and_then(|path| crate::events::latest_since(&path, 0)),
        now: now_millis(),
    })
}

/// The panel, from the facts (`docs/hook-setup.md` §5, FR-55).
///
/// **In repair order**, which is not the order the checks were written in: the helper comes
/// first because nothing below it can be fixed while it is missing — entries naming a program
/// that is not there install cleanly and report nothing — and the file comes before what is
/// in it for the same reason.
#[must_use]
pub fn diagnose(facts: &Facts<'_>) -> Diagnosis {
    let installed = facts.settings.installed();
    Diagnosis {
        mode: crate::events::mode(installed != Installed::No, facts.last_event, facts.now).label(),
        checks: vec![
            check_helper(facts.helper, facts.helper_found),
            check_settings(facts.settings, facts.settings_path),
            check_entries(facts.settings, facts.helper_found),
            check_events(installed, facts.last_event, facts.now),
        ],
    }
}

/// The one problem this product cannot write its way out of.
fn check_helper(helper: Option<&std::path::Path>, found: bool) -> Check {
    let expected = helper.map_or_else(
        || "beside this app".to_owned(),
        |path| path.display().to_string(),
    );
    Check {
        id: "helper",
        what: "The small program that does the reporting",
        ok: found,
        // The path only where the path is the point. When the program is there, quoting
        // where it is costs three wrapped lines and tells the user nothing they can act on —
        // and it pushed the events row, which is the row the other three exist for, below the
        // fold. When it is *missing*, the path is the whole message.
        detail: if found {
            "Found beside this app, where it is installed.".to_owned()
        } else {
            format!("Not at {expected}, where it should be.")
        },
        // No button, deliberately. Every other row offers something this app can do; putting
        // its own missing file back is not one of them, and a button that could only report a
        // failure would be worse than the sentence.
        action: (!found).then_some(Action {
            kind: "tell",
            label: "Reinstalling this app puts it back. Nothing here can.",
        }),
    }
}

/// Whether the file can be worked with at all (FR-53).
fn check_settings(settings: &SettingsFile, path: &str) -> Check {
    let what = "Claude Code's settings file";
    match settings {
        // Not a fault. A first install writes the file Claude Code would have written itself,
        // and marking this row broken would send the user off to open a file that is not
        // there — while the row below already offers the thing that creates it.
        SettingsFile::Missing => Check {
            id: "settings",
            what,
            ok: true,
            detail: format!("Not there yet. Setting up would create {path}."),
            action: None,
        },
        SettingsFile::Unreadable(why) => Check {
            id: "settings",
            what,
            ok: false,
            detail: format!("{path} cannot be read: {why}. Nothing has been written to it."),
            action: Some(Action {
                kind: "open-settings",
                label: "Open the file",
            }),
        },
        SettingsFile::Read(_) => Check {
            id: "settings",
            what,
            ok: true,
            detail: format!("Read from {path}."),
            action: None,
        },
    }
}

/// Whether our own entries are in it, and whether they are this build's.
fn check_entries(settings: &SettingsFile, helper_found: bool) -> Check {
    // Offered only where it could actually succeed. Setting up with the helper missing would
    // write entries naming a program that is not there — `hook_preview` refuses it anyway — so
    // this row states what is true and leaves the row above to be the one that is acted on.
    let repair = |label: &'static str| {
        helper_found.then_some(Action {
            kind: "install",
            label,
        })
    };
    let (ok, detail, action) = match settings {
        SettingsFile::Unreadable(_) => (
            false,
            "Cannot be checked while the file above cannot be read.".to_owned(),
            None,
        ),
        SettingsFile::Missing | SettingsFile::Read(Installed::No) => (
            false,
            "None of them are there.".to_owned(),
            repair("Set it up…"),
        ),
        SettingsFile::Read(Installed::Other) => (
            false,
            "Entries of ours are there, but not this version's set — an older install, or a \
             partial removal."
                .to_owned(),
            repair("Repair them…"),
        ),
        SettingsFile::Read(Installed::Current) => (true, "All of them are there.".to_owned(), None),
    };
    Check {
        id: "entries",
        what: "This app's entries in that file",
        ok,
        detail,
        action,
    }
}

/// Whether anything is actually reporting in (FR-52).
///
/// The row the other three exist for. Entries that are installed and silent buy the user
/// nothing, and a panel that stopped at "installed" would report that situation as success —
/// the single failure this whole screen is against.
fn check_events(installed: Installed, last: Option<i64>, now: i64) -> Check {
    let quiet = crate::events::QUIET_AFTER_MILLIS;
    let since = |at: i64| ago(now.saturating_sub(at));

    // **Nothing installed is judged first, whatever the log says.** The event log outlives the
    // entries that filled it: take them out and a report from a minute ago is still sitting
    // there. Reading that as "reports are arriving" would have this row agree with itself
    // against the row above — a panel saying the entries are gone and the reporting is fine.
    let (ok, detail, action) = if installed == Installed::No {
        (
            false,
            match last {
                Some(at) => format!(
                    "Nothing can be reporting in with no entries in place. The last report \
                     was {}, from before they went.",
                    since(at)
                ),
                None => "Nothing has ever reported in, which is what you would expect with \
                         no entries in place."
                    .to_owned(),
            },
            // The repair belongs to the entries row. Two rows offering the same button is how
            // "the single action" stops being single.
            None,
        )
    } else {
        match last {
            Some(at) if now.saturating_sub(at) <= quiet => {
                (true, format!("The last one arrived {}.", since(at)), None)
            }
            // Silent, with entries in place. Two causes, and nothing here can tell them
            // apart: no session has started since the entries went in, or something is
            // stopping the helper. **One action separates them** — start a session and send
            // something. The window then waits and says which it was, instead of guessing on
            // the user's behalf.
            Some(at) => (
                false,
                format!(
                    "Nothing for {} — longer than the {} minutes after which the board stops \
                     calling itself exact.",
                    since(at),
                    quiet / 60_000
                ),
                Some(WATCH),
            ),
            None => (
                false,
                "The entries are in place, but nothing has ever reported in.".to_owned(),
                Some(WATCH),
            ),
        }
    };
    Check {
        id: "events",
        what: "Reports arriving from your sessions",
        ok,
        detail,
        action,
    }
}

/// The action for silence: watch for the next event, rather than assert a cause.
const WATCH: Action = Action {
    kind: "watch",
    label: "Watch for the next one",
};

/// A duration in the coarsest unit that still says something useful.
///
/// Never seconds past a minute and never minutes past an hour. This is read to answer "is it
/// arriving, or has it stopped", and `1 h 24 m 09 s` is precision about a question nobody
/// asked.
fn ago(millis: i64) -> String {
    let seconds = millis / 1_000;
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match seconds {
        ..=1 => "just now".to_owned(),
        s if s < 60 => plural(s, "second"),
        s if s < 3_600 => plural(s / 60, "minute"),
        s if s < 86_400 => plural(s / 3_600, "hour"),
        s => plural(s / 86_400, "day"),
    }
}

/// Whether a real event has arrived since the entries were written (FR-50).
#[tauri::command]
#[must_use]
pub fn hook_events_seen(since: i64) -> bool {
    crate::events::events_path()
        .and_then(|path| crate::events::latest_since(&path, since))
        .is_some()
}

#[tauri::command]
#[must_use]
pub fn hook_settings_path() -> String {
    settings_path()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

/// Opens the settings file in whatever the user opens `.json` files with (FR-53's action).
///
/// Through the shell's own file association rather than a text editor of our choosing: this
/// is the user's file, and which program edits it is their setting, not ours.
#[tauri::command]
pub fn open_settings_file() -> Result<(), String> {
    let path = settings_path().ok_or("this computer does not say where the settings live")?;
    #[cfg(windows)]
    let started = std::process::Command::new("cmd")
        .args(["/c", "start", ""])
        .arg(&path)
        // No console for the moment `cmd` takes to hand the file on.
        .creation_flags(crate::win::NO_CONSOLE)
        .spawn();
    #[cfg(not(windows))]
    let started = std::process::Command::new("xdg-open").arg(&path).spawn();

    started.map(|_| ()).map_err(|e| e.to_string())
}

/// Opens the setup window, or brings it forward if it is already open.
///
/// The **one** place in this product that asks for the foreground, and the exception proves
/// the rule: FR-30 keeps the board out of the user's way because the board is ambient. This
/// window was asked for by a click and has a question to ask.
#[tauri::command]
pub fn open_hook_setup(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    open_screen(&app, Screen::Offer)
}

/// Creates the window on one of its screens.
///
/// One builder, so the entry points cannot drift into differently shaped windows.
fn open_at(app: &tauri::AppHandle, screen: Screen) -> Result<(), String> {
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(screen.url().into()))
        .title(screen.title())
        .inner_size(560.0, 560.0)
        // Centred, not placed: the board lives in a corner and is always on top, so a window
        // opened near it would open underneath it.
        .center()
        .resizable(true)
        .always_on_top(false)
        .skip_taskbar(false)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Opens the setup window on the diagnosis rather than on the offer (FR-55).
///
/// The step is carried in the URL, so a window that is not open yet paints the right screen on
/// its first frame rather than flashing the wrong one. A window that **is** open cannot be
/// navigated without throwing away whatever the user is part-way through, so it is told
/// instead, and it decides.
///
/// # Errors
///
/// Whatever the window builder says.
#[tauri::command]
pub fn open_hook_diagnosis(app: tauri::AppHandle) -> Result<(), String> {
    open_screen(&app, Screen::Diagnosis)
}

/// Opens the setup window on the explanation behind revealing a session (ADR-0029).
///
/// # Errors
///
/// Whatever the window builder says.
#[tauri::command]
pub fn open_reveal_setup(app: tauri::AppHandle) -> Result<(), String> {
    open_screen(&app, Screen::Reveal)
}

/// The screen a command-line flag asks for, if it asks for one.
///
/// **One mapping, used twice.** A first launch reads it while parsing its arguments; a second
/// launch reads it out of the arguments the running board is handed. Written down once so the
/// two cannot drift into a flag that works only when the board is not already running, which
/// is the shape the defect took: `--shortcuts` opened nothing and summoned the board instead.
#[must_use]
pub fn screen_for_flag(flag: &str) -> Option<Screen> {
    match flag {
        "--setup" => Some(Screen::Offer),
        "--diagnose" => Some(Screen::Diagnosis),
        "--reveal-setup" => Some(Screen::Reveal),
        "--shortcuts" => Some(Screen::Shortcuts),
        _ => None,
    }
}

/// Shows the screen a second launch asked for, and reports whether it asked for one.
///
/// A launch carrying one of these flags wants that screen, not the board: it is the same
/// request the menu makes, arriving by another route. Anything else is a plain second launch
/// and is answered with a summons.
pub fn open_requested(app: &tauri::AppHandle, argv: &[String]) -> bool {
    let Some(screen) = argv.iter().find_map(|arg| screen_for_flag(arg)) else {
        return false;
    };
    let _ = open_screen(app, screen);
    true
}

/// Shows one of the window's screens, opening the window if it is not already there.
fn open_screen(app: &tauri::AppHandle, screen: Screen) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_title(screen.title());
        let _ = window.set_focus();
        let _ = window.emit("step", screen.step());
        return Ok(());
    }
    open_at(app, screen)
}

/// Whether a click also brings the session's own tab to the front.
#[tauri::command]
#[must_use]
pub fn reveal_state(state: tauri::State<'_, App>) -> bool {
    state
        .settings
        .lock()
        .is_ok_and(|settings| settings.reveal_session)
}

/// Turns that on or off, and remembers it (FR-28).
#[tauri::command]
pub fn reveal_set(on: bool, app: tauri::AppHandle) {
    crate::app::set_reveal(&app, on);
}

/// The two keys, and what became of each of them the last time they were offered.
#[derive(Debug, Clone, Serialize)]
pub struct Shortcuts {
    pub summon: String,
    pub toggle: String,
    pub summon_bound: crate::hotkeys::Bound,
    pub toggle_bound: crate::hotkeys::Bound,
}

/// What the shortcuts screen draws itself from.
#[tauri::command]
#[must_use]
pub fn shortcut_state(state: tauri::State<'_, App>) -> Shortcuts {
    let keys = state.settings.lock().map_or_else(
        |_| crate::settings::Hotkeys::default(),
        |settings| settings.hotkeys.clone(),
    );
    let bound = state
        .hotkeys
        .lock()
        .map_or_else(|_| crate::hotkeys::Registered::default(), |held| *held);
    Shortcuts {
        summon: keys.summon,
        toggle: keys.toggle,
        summon_bound: bound.summon,
        toggle_bound: bound.toggle,
    }
}

/// Changes the keys, remembers them, and **offers them to the operating system straight away**.
///
/// The answer comes back from the same call that made the change, because the question the
/// user is actually asking — *can I have this key?* — has no answer that can be worked out
/// from the key. Something else on the machine either holds it or does not, and the only way
/// to find out is to ask for it. So the screen shows what came back rather than what was
/// intended (FR-61, FR-52's argument again).
///
/// # Errors
///
/// Never returns `Err`: an accelerator that cannot be parsed is a **result**, reported in the
/// state as `unreadable`, not a failure of the command. The signature keeps the `Result` that
/// every other command in this window has, so the front end has one shape to handle.
#[tauri::command]
pub fn shortcut_set(
    summon: String,
    toggle: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, App>,
) -> Result<Shortcuts, String> {
    {
        let Ok(mut settings) = state.settings.lock() else {
            return Err("the settings are in use".to_owned());
        };
        settings.hotkeys.summon = summon.trim().to_owned();
        settings.hotkeys.toggle = toggle.trim().to_owned();
    }
    crate::app::persist_settings(&app);

    let registered = crate::hotkeys::register(&app);
    if let Ok(mut held) = state.hotkeys.lock() {
        *held = registered;
    }
    // Both menus state the keys, and the tray's copy is one the shell will not ask us for
    // again before it opens.
    crate::tray::refresh_menu(&app, &state);

    Ok(shortcut_state(state))
}

/// Opens the setup window on the shortcuts screen.
///
/// # Errors
///
/// Whatever the window builder says.
#[tauri::command]
pub fn open_shortcuts_setup(app: tauri::AppHandle) -> Result<(), String> {
    open_screen(&app, Screen::Shortcuts)
}

#[tauri::command]
pub fn close_setup(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
}
