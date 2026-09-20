//! Running the window: what the board does as an application rather than as a picture.
//!
//! Three obligations live here, and each is a requirement rather than a nicety.
//!
//! * **It never takes the foreground** (FR-30). A status board that costs the user a
//!   keystroke is worse than no status board, so the window is created unfocused and nothing
//!   below asks for focus afterwards.
//! * **It stays above the editors it watches** (FR-26), which is what makes it glanceable
//!   without being managed.
//! * **It remembers where it was and what was folded** (FR-28), and comes back somewhere the
//!   user can actually see (TC-68).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use mcv_core::palette::Theme;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};

use crate::demo;
use crate::raise::Outcome;
use crate::settings::{Area, Settings, clamp_to_displays, path_in};
use crate::view::{BoardView, Inspect};
use crate::watch::live::{Watcher, claude_dir, now_millis};

/// Everything the running board holds.
pub struct App {
    pub theme: Theme,
    pub inspect: Inspect,
    pub settings: Mutex<Settings>,
    pub board: Mutex<BoardView>,
    /// Present when the board is watching real sessions rather than the recordings.
    ///
    /// **Only the polling thread locks this.** It holds the lock while it reads directories,
    /// tails transcripts and refreshes processes, so a command that waited on it would put a
    /// click behind a disk. Commands use the two snapshots below instead.
    pub watcher: Option<Mutex<Watcher>>,
    /// Each session's process, published by the poll for clicks to use.
    pub sessions: Mutex<HashMap<String, (u32, Option<u32>)>>,
    /// Where the editor lock files are.
    pub ide_dir: PathBuf,
    /// What became of each global shortcut.
    ///
    /// Kept so the menu can say which one did not take, and why. A shortcut another
    /// application has taken is a feature that silently does nothing, and the board's whole
    /// argument is that it does not present something it is not delivering (FR-52's
    /// reasoning, applied here). **Per key**, because the menu shows one line per key and a
    /// single verdict for the pair marked a working shortcut as broken.
    pub hotkeys: Mutex<crate::hotkeys::Registered>,
    /// The width the renderer measured for a folded board, and the set of workspaces it
    /// measured it for.
    ///
    /// Held here rather than in the view because the view is rebuilt from scratch on every
    /// poll: keeping it there meant each poll erased the measurement and pushed the window
    /// back to 300, and the renderer — which only reports when the workspaces change — never
    /// said it again.
    pub folded_width: Mutex<Option<(String, u32)>>,
    /// The whereabouts line the tray menu currently carries (§6.3).
    ///
    /// The menu cannot be rebuilt when it opens — the shell opens it without asking us — so
    /// the line is replaced when it stops being true. A drag is the case that needs this and
    /// the case that cannot announce itself: `Moved` fires per pixel, and rebuilding a menu
    /// that often would cost more than the line is worth. So the rescue's own tick compares
    /// the line it would write against this one, and replaces the menu only when they differ.
    pub last_where: Mutex<String>,
}

/// How often the board looks. NFR-01 allows two seconds end to end for a status change to
/// appear, and this is the share of it spent waiting rather than working.
const POLL: std::time::Duration = std::time::Duration::from_millis(750);

/// Opens the window and runs until it closes.
///
/// # Panics
///
/// If the window cannot be created at all, which is not a condition the board can degrade
/// around: there is nothing to show and nowhere to show it.
pub fn run(theme: Theme, inspect: Inspect, live: bool, open_setup: Option<crate::setup::Screen>) {
    tauri::Builder::default()
        // **First, before anything else is set up.** A second copy of the board is not a
        // second board: it is one more tray icon, one more writer of the same settings file,
        // and a process that cannot register the shortcuts the first one already holds — which
        // the user reads as "my keys stopped working", with nothing on screen to say why
        // (FR-60, [ADR-0032](../../../docs/adr/0032-one-board-per-machine.md)).
        //
        // What the second launch does instead is **ask for the board**. Someone who double
        // clicks the icon of a resident tool that is already resident wants to see it, and
        // that is the same request the tray icon and the shortcut make — so it is answered the
        // same way, by the same summons, in the same corner.
        //
        // **Nothing in this callback may panic.** On Windows it runs inside the plugin's hidden
        // window procedure while the second process waits in `SendMessageW` for it to return,
        // and a panic unwinding through a Win32 callback boundary would take the first board —
        // the one being used — down with the launch that was only asking to see it.
        // `summon_board` is written for that: every lookup is a `let … else` or an `unwrap_or`,
        // and a board that cannot be found is a summons that quietly does nothing.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // **The arguments are part of the request.** A launch carrying `--shortcuts` wants
            // that screen, not the board, and dropping them made every one of those flags work
            // only while no board was running — which is to say, never, for the person who has
            // one open and wants to change a key.
            if crate::setup::open_requested(app, &argv) {
                return;
            }
            crate::hotkeys::summon_board(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            current_board,
            toggle_fold,
            raise_session,
            report_folded_width,
            // The hook setup window (docs/hook-setup.md §4). None of these runs unless the
            // user opened that window and pressed something in it (FR-54).
            crate::setup::hook_state,
            crate::setup::hook_preview,
            crate::setup::hook_install,
            crate::setup::hook_events_seen,
            crate::setup::hook_diagnosis,
            crate::setup::hook_settings_path,
            crate::setup::open_settings_file,
            crate::setup::open_hook_setup,
            crate::setup::open_hook_diagnosis,
            crate::setup::open_reveal_setup,
            crate::setup::reveal_state,
            crate::setup::reveal_set,
            crate::setup::shortcut_state,
            crate::setup::shortcut_set,
            crate::setup::open_shortcuts_setup,
            crate::setup::close_setup,
            open_menu
        ])
        .on_menu_event(|app, event| {
            crate::menu::clicked(app, event.id().as_ref());
        })
        .setup(move |app| {
            let config_dir = app.path().app_config_dir()?;
            let settings = Settings::load(&path_in(&config_dir));

            let mut watcher = live
                .then(claude_dir)
                .flatten()
                .map(|claude| Watcher::new(claude, theme, inspect));
            let mut board = match watcher.as_mut() {
                Some(watcher) => {
                    let folded = settings.folded.clone();
                    watcher.poll(now_millis(), &|key| folded.contains(key))
                }
                None => demo::board(theme, inspect, &settings),
            };
            // Before the window is sized: the first frame is drawn at the size the user chose
            // last time, not at the design's own and then jumped.
            board.scale = settings.scale();

            let sessions = watcher
                .as_ref()
                .map(Watcher::session_processes)
                .unwrap_or_default();
            let ide_dir = watcher
                .as_ref()
                .map_or_else(|| PathBuf::from("."), Watcher::ide_dir);

            let (width, height) = (board.width(), board.height());
            app.manage(App {
                theme,
                inspect,
                settings: Mutex::new(settings),
                board: Mutex::new(board),
                watcher: watcher.map(Mutex::new),
                sessions: Mutex::new(sessions),
                ide_dir,
                folded_width: Mutex::new(None),
                hotkeys: Mutex::new(crate::hotkeys::Registered::default()),
                last_where: Mutex::new(String::new()),
            });

            if live {
                start_watching(app.handle().clone());
            }

            // Registered after the state exists, because the handlers read the settings out of
            // it. Whether it worked is remembered rather than logged: the menu is where the
            // user finds out, and a key that silently does nothing is the one outcome this
            // must not have.
            let registered = crate::hotkeys::register(app.handle());
            if let Some(state) = app.try_state::<App>() {
                if let Ok(mut held) = state.hotkeys.lock() {
                    *held = registered;
                }
            }

            // After the shortcuts, because the tray carries the same menu and that menu says
            // whether they registered. A tray that could not be created is not a reason to
            // take the board down — it is yesterday's board, which worked.
            if let Some(state) = app.try_state::<App>() {
                if let Err(e) = crate::tray::build(app.handle(), &state) {
                    eprintln!("mcv-board: no tray icon: {e}");
                }
            }

            if let Some(window) = app.get_webview_window("board") {
                fit(&window, width, height);
                restore_position(app.handle(), &window);
            }

            // Development only, and the reason it exists is the screenshot loop: both of the
            // setup window's screens are reached from the board's menu, which a capture
            // script cannot open. It opens the window and nothing else — every step inside it
            // still needs the user (FR-54).
            match open_setup {
                Some(crate::setup::Screen::Diagnosis) => {
                    let _ = crate::setup::open_hook_diagnosis(app.handle().clone());
                }
                Some(crate::setup::Screen::Reveal) => {
                    let _ = crate::setup::open_reveal_setup(app.handle().clone());
                }
                Some(crate::setup::Screen::Shortcuts) => {
                    let _ = crate::setup::open_shortcuts_setup(app.handle().clone());
                }
                Some(crate::setup::Screen::Offer) => {
                    let _ = crate::setup::open_hook_setup(app.handle().clone());
                }
                None => {}
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Dragged onto a display with a different scaling factor: everything is measured
            // in CSS px at 100 %, so the window has to be re-measured too (FR-29, TC-69).
            if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event {
                let app = window.app_handle();
                if let (Some(state), Some(view)) =
                    (app.try_state::<App>(), app.get_webview_window("board"))
                {
                    if let Ok(board) = state.board.lock() {
                        // The event's factor, not the window's. The window is still answering
                        // with the old one at this point, and a board sized for the display it
                        // just left is the "it went to pieces when I plugged a monitor in"
                        // report this came from.
                        fit_at(&view, board.width(), board.height(), *scale_factor);
                    }
                }
            }
            // Remembering where the board is means noticing when it moves. Written on every
            // move rather than on exit, because a board that is killed rather than closed —
            // which is how a resident tool usually ends — would otherwise forget.
            if let WindowEvent::Moved(position) = event {
                let app = window.app_handle();
                if let Some(state) = app.try_state::<App>() {
                    // Take a copy under the lock and write it outside. Writing a file can
                    // block for a surprisingly long time — a roaming profile, an antivirus
                    // scanner, a locked directory — and a board that holds a lock across
                    // that stalls every other event behind a disk.
                    let snapshot = {
                        let Ok(mut settings) = state.settings.lock() else {
                            return;
                        };
                        settings.position = Some((position.x, position.y));
                        settings.clone()
                    };
                    persist(app, &snapshot);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("the board could not start");
}

/// Sizes the window to the board.
///
/// In **physical** pixels, computed from the window's own scale factor rather than handed to
/// Tauri as a logical size. The two are not the same thing at the moment of startup: the
/// window's scale factor is not settled until it has been placed, and a logical size
/// converted with a stale factor produced a board whose rows were the wrong height — 375
/// physical pixels wide where 525 was wanted, on a 175 % display.
fn fit(window: &WebviewWindow, width: u32, height: u32) {
    fit_at(window, width, height, window.scale_factor().unwrap_or(1.0));
}

/// [`fit`], for the one moment the window cannot be asked its own scale factor.
///
/// **A scale change hands you the new factor; the window does not have it yet.** Asking
/// `scale_factor()` from inside `ScaleFactorChanged` can still answer with the old one, and on
/// this desk — a 100 % display beside a 175 % one — sizing 300 × 310 CSS px with a stale 1.0
/// makes a window 225 px too narrow and 233 px too short for what the renderer then draws in
/// it. What the user sees is a board with its title strip and nothing else: not a board that
/// failed to draw, but a board drawn correctly into a window measured for the wrong screen.
fn fit_at(window: &WebviewWindow, width: u32, height: u32, scale: f64) {
    // Read before the resize: whether the board was wholly on a display is the whole question
    // below, and afterwards it is too late to ask.
    let was = window.outer_size().map(|size| (size.width, size.height));
    let position = window.outer_position().map(|at| (at.x, at.y));

    let physical = |value: u32| (f64::from(value) * scale).round() as u32;
    let (width, height) = (physical(width), physical(height));
    let _ = window.set_size(PhysicalSize::new(width, height));

    // Growing is how the board walks off a screen: it grows right and down from its top-left,
    // and a board summoned to a corner has only the 40 px inset in that direction (§5). Undo
    // what the resize did, and nothing else — a board its owner parked over an edge was not
    // put there by a resize and is not brought back by one.
    let (Ok(was), Ok(position)) = (was, position) else {
        return;
    };
    let displays = work_areas(window);
    if let Some((x, y)) = crate::place::contain_growth(position, was, (width, height), &displays) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// A monitor's usable rectangle: what is left of it once the taskbar has taken its share.
///
/// The work area rather than the bounds, everywhere the board is placed. A board summoned to
/// the corner of the *bounds* is a board partly under the taskbar, which on a window that
/// cannot be dragged out from under one is the same as losing it.
#[must_use]
pub fn work_area(monitor: &tauri::window::Monitor) -> Area {
    let rect = monitor.work_area();
    Area {
        x: rect.position.x,
        y: rect.position.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

/// Every display's work area, in the order the platform lists them.
///
/// The order is what the tray menu counts to say "display 2", so it is the platform's own
/// numbering and not one this product invents.
#[must_use]
pub fn work_areas(window: &WebviewWindow) -> Vec<Area> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(work_area)
        .collect()
}

/// The board's outer size, or the size it is configured to open at.
///
/// The fallback matters on the one call that happens before the window has been laid out.
#[must_use]
pub fn window_size(window: &WebviewWindow) -> (u32, u32) {
    window
        .outer_size()
        .map_or((300, 310), |size| (size.width, size.height))
}

/// Brings the board back if it is somewhere nobody can reach it (FR-28).
///
/// Called from the poll rather than from an event, because there is no event: a display that
/// is unplugged, put to sleep, or taken away by a remote session does not always move the
/// windows that were on it, and a frameless window with no taskbar button is then gone for
/// good — the user's only remaining route to it is the tray icon, and until 2026-09-06 that
/// did not move it either.
///
/// It does nothing at all in the ordinary case, and that is deliberate: [`crate::place`]'s
/// rule is *reachable*, not *fully on screen*, so a board straddling two displays or parked
/// half over an edge is left exactly where its owner put it.
fn rescue_if_lost(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("board") else {
        return;
    };
    // A hidden board is not lost, and moving one would silently undo where it came back to.
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(position) = window.outer_position() else {
        return;
    };
    let displays = work_areas(&window);
    let size = window_size(&window);
    let mut place = (position.x, position.y);

    if let Some((x, y)) = crate::place::rescue(place, size, &displays) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let _ = window.set_always_on_top(true);
        // The same flash a summons ends with, and for the same reason: the board has just
        // arrived somewhere the user was not looking (§6.3).
        let _ = app.emit("flash", ());
        // Where it ended up, not where it was found: the menu below has to describe the board
        // the user will now go looking for.
        place = (x, y);
    }

    // Whether or not anything was rescued, this is the one place that holds a fresh position
    // and a fresh list of displays — so it is where the tray's whereabouts line is checked
    // against what the board is actually doing. A board dragged to another display says
    // nothing to the tray otherwise, and the line would go on naming the display it left.
    refresh_where(app, place, size, &displays);
}

/// Replaces the tray menu when the line that says where the board is stops being true.
///
/// Compared as the rendered line rather than as a [`crate::place::Whereabouts`], because the
/// line is what the user reads: two whereabouts that say the same sentence need no new menu.
///
/// **Only ever called for a board that is on screen.** Its one caller returns before this
/// when the window is hidden, which is why the visibility handed to `whereabouts` is a
/// constant here. Hiding and showing replace the menu on their own paths, where the answer is
/// *hidden* and no position is consulted at all.
fn refresh_where(app: &tauri::AppHandle, place: (i32, i32), size: (u32, u32), displays: &[Area]) {
    let line = crate::place::where_line(crate::place::whereabouts(true, place, size, displays));
    let Some(state) = app.try_state::<App>() else {
        return;
    };
    let Ok(mut held) = state.last_where.lock() else {
        return;
    };
    if *held == line {
        return;
    }
    held.clone_from(&line);
    drop(held);
    crate::tray::refresh_menu_from(app);
}

/// Puts the window back where it was, if that place still exists.
fn restore_position(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    let Some(state) = app.try_state::<App>() else {
        return;
    };
    let Ok(settings) = state.settings.lock() else {
        return;
    };
    let Some(remembered) = settings.position else {
        return;
    };

    let displays: Vec<Area> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| Area {
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
        })
        .collect();

    let size = window
        .outer_size()
        .map(|s| (s.width, s.height))
        .unwrap_or((300, 200));

    if let Some((x, y)) = clamp_to_displays(remembered, size, &displays) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Writes the settings as they stand now.
///
/// For callers that have just changed them through the state rather than holding a snapshot —
/// the shortcuts screen is the one that does. Taken under the lock and written outside it, for
/// the reason [`persist`] gives.
pub fn persist_settings(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<App>() else {
        return;
    };
    let Ok(settings) = state.settings.lock().map(|held| held.clone()) else {
        return;
    };
    persist(app, &settings);
}

fn persist(app: &tauri::AppHandle, settings: &Settings) {
    let Ok(config_dir) = app.path().app_config_dir() else {
        return;
    };
    if let Err(e) = settings.save(&path_in(&config_dir)) {
        // Losing the remembered position is a nuisance, not a fault. The board keeps running.
        eprintln!("mcv-board: could not save settings: {e}");
    }
}

/// Opens the board's menu where the pointer is (§6).
///
/// Built fresh on every right-click: it shows the current mode and the current size, and a
/// menu built once would be wrong the second time it was opened.
#[tauri::command]
fn open_menu(app: tauri::AppHandle, state: tauri::State<'_, App>) {
    let Ok(menu) = crate::menu::build(&app, &state, crate::menu::Opened::OnTheBoard) else {
        return;
    };
    if let Some(window) = app.get_webview_window("board") {
        // The one moment the board asks for focus, and the same exception the setup window
        // makes: FR-30 keeps it out of the way while it *updates*, and this is a right-click —
        // the user is talking to it. Without the focus the menu appears and then ignores every
        // click, because a popup menu tracks input for the window that owns it and this window
        // is never the active one.
        let _ = window.set_focus();
        let _ = window.popup_menu(&menu);
    }
}

/// Draws the board at a different size, and remembers it (FR-28, §2.8).
///
/// The measured folded width goes with it: it was measured for a board of another size, and
/// keeping it would leave the window sized for the picture it used to draw.
pub fn set_scale(app: &tauri::AppHandle, percent: u32) {
    let Some(state) = app.try_state::<App>() else {
        return;
    };

    let snapshot = {
        let Ok(mut settings) = state.settings.lock() else {
            return;
        };
        settings.scale = percent;
        settings.clone()
    };
    persist(app, &snapshot);
    if let Ok(mut measured) = state.folded_width.lock() {
        *measured = None;
    }

    let sized = {
        let Ok(mut board) = state.board.lock() else {
            return;
        };
        board.scale = snapshot.scale();
        board.folded_width = None;
        (board.width(), board.height(), board.clone())
    };
    if let Some(window) = app.get_webview_window("board") {
        fit(&window, sized.0, sized.1);
    }
    // The tray's copy of the menu ticks the current size, and the shell will not ask us for
    // it again before it opens.
    crate::tray::refresh_menu(app, &state);
    let _ = app.emit("board", sized.2);
}

/// Turns the reveal on or off, remembers it, and updates the menus that state it.
pub fn set_reveal(app: &tauri::AppHandle, on: bool) {
    let Some(state) = app.try_state::<App>() else {
        return;
    };
    let snapshot = {
        let Ok(mut settings) = state.settings.lock() else {
            return;
        };
        settings.reveal_session = on;
        settings.clone()
    };
    persist(app, &snapshot);
    // The tray's copy of the menu ticks this, and the shell will not ask us for it again.
    crate::tray::refresh_menu(app, &state);
}

/// Hands the front end the board to draw.
#[tauri::command]
fn current_board(state: tauri::State<'_, App>) -> BoardView {
    state.board.lock().map_or_else(
        |poisoned| poisoned.into_inner().clone(),
        |board| board.clone(),
    )
}

/// Folds or unfolds a workspace, and remembers which (FR-28, TC-56b).
///
/// The board's top-left corner does not move: it grows and shrinks downwards (TC-55), which
/// is why only the height is set here.
#[tauri::command]
fn toggle_fold(key: String, app: tauri::AppHandle, state: tauri::State<'_, App>) {
    let now_folded = {
        let Ok(mut settings) = state.settings.lock() else {
            return;
        };
        let folded = !settings.is_folded(&key);
        settings.set_folded(&key, folded);
        let snapshot = settings.clone();
        drop(settings);
        persist(&app, &snapshot);
        folded
    };

    // Applied to the board already on screen. Folding changes how the same sessions are
    // drawn, not what they are, so there is nothing to ask the observer for — and a click
    // that went to the observer would queue behind whatever the polling thread was reading.
    let sized = {
        let Ok(mut board) = state.board.lock() else {
            return;
        };
        board.set_folded(&key, now_folded);
        board.folded_width = folded_width_for(&state, &board);
        (board.width(), board.height(), board.clone())
    };

    if let Some(window) = app.get_webview_window("board") {
        fit(&window, sized.0, sized.1);
    }
    let _ = app.emit("board", sized.2);
}

/// The measured folded width, if it was measured for the workspaces now on the board.
fn folded_width_for(state: &tauri::State<'_, App>, board: &BoardView) -> Option<u32> {
    if !board.fully_folded {
        return None;
    }
    let signature = board.workspace_signature();
    state
        .folded_width
        .lock()
        .ok()?
        .as_ref()
        .and_then(|(measured_for, width)| (*measured_for == signature).then_some(*width))
}

/// Brings the VS Code window that owns a session to the front (FR-33).
///
/// The answer is sent back to the front end whatever it is. A raise that fails has to be
/// **visible** — TC-75 fails a silent no-op — and the reason it can be reported at all is
/// that the outcome is measured rather than trusted: the return values lie, so the chain
/// reads the foreground window back before saying it worked
/// ([ADR-0009](../../../docs/adr/0009-map-a-session-to-its-window.md)).
#[tauri::command]
fn raise_session(id: String, app: tauri::AppHandle, state: tauri::State<'_, App>) {
    let raised = raise_for(&id, &state);
    let _ = app.emit(
        "raised",
        RaiseResult {
            id,
            outcome: match raised.outcome {
                Outcome::Raised => "raised",
                Outcome::NotFound => "not-found",
                Outcome::Refused => "refused",
            },
        },
    );
}

/// What the front end is told about a click.
#[derive(Clone, serde::Serialize)]
struct RaiseResult {
    id: String,
    outcome: &'static str,
}

#[cfg(windows)]
fn raise_for(id: &str, state: &tauri::State<'_, App>) -> crate::raise::Raise {
    // From the snapshot the poll publishes, never from the watcher: the polling thread holds
    // that lock while it reads directories and tails transcripts, and a click must not wait
    // behind a disk.
    let Some((_, parent)) = state
        .sessions
        .lock()
        .ok()
        .and_then(|sessions| sessions.get(id).copied())
    else {
        // The demonstration board's sessions are recordings, not processes.
        return NOTHING_RAISED;
    };
    let ide = state.ide_dir.clone();

    let Some(host) = parent else {
        return NOTHING_RAISED;
    };

    // A table of its own, rather than the watcher's. Raising waits a quarter of a second to
    // see whether the window came, and holding the watcher's lock across that would stall a
    // poll behind a click. Building one is cheap: it refreshes only the handful of processes
    // the walk actually asks about.
    let mut table = crate::watch::process::ProcessTable::new();

    // The VS Code instance is the extension host's own parent, and every window of that
    // instance is drawn by a process descended from it. Asking that question is what keeps a
    // browser window whose title happens to name the workspace out of the answer.
    let Some(editor) = table.parent_of(host) else {
        return NOTHING_RAISED;
    };

    // The instance's *other* extension hosts — one per other window of the same editor. They
    // are what names this window when it has no folder of its own to be named by: their
    // folders account for every window but one (ADR-0028).
    let mut siblings: Vec<u32> = state
        .sessions
        .lock()
        .map(|sessions| {
            sessions
                .values()
                .filter_map(|(_, parent)| *parent)
                .filter(|other| *other != host)
                .collect()
        })
        .unwrap_or_default();
    siblings.sort_unstable();
    siblings.dedup();
    siblings.retain(|other| table.parent_of(*other) == Some(editor));

    let raised = crate::raise::raise_window_of(parent, &ide, &siblings, &mut |owner| {
        table.is_descendant_of(owner, editor)
    });
    reveal_session(id, &raised, host, editor, &ide, state, &mut table);
    raised
}

#[cfg(not(windows))]
fn raise_for(_id: &str, _state: &tauri::State<'_, App>) -> crate::raise::Raise {
    // Raising another application's window is the one genuinely platform-specific thing the
    // board does (ADR-0002 targets Windows first). Elsewhere it says so rather than pretending.
    NOTHING_RAISED
}

/// Nothing was found, so nothing was raised.
const NOTHING_RAISED: crate::raise::Raise = crate::raise::Raise {
    outcome: Outcome::NotFound,
    window: None,
};

/// Brings the clicked session's own tab to the front, when the user has asked for that.
///
/// **Everything about this is conditional, and each condition is load-bearing** (ADR-0029):
///
/// * the user turned it on, and it is off until they do;
/// * the window was raised and the raise was *verified* — a URL sent at a window that never
///   came forward lands somewhere else;
/// * that window is **still** the foreground one at the moment of sending, because the editor
///   hands a URL to whichever of its windows is active and this is the only lever there is;
/// * and the editor is one whose URL scheme this build knows, because guessing would hand the
///   URL to a different application.
///
/// Any of them failing means the click did what it always did: the window came forward, and
/// nothing else happened. That is the failure this is allowed to have.
#[cfg(windows)]
fn reveal_session(
    id: &str,
    raised: &crate::raise::Raise,
    host: u32,
    editor: u32,
    ide_dir: &std::path::Path,
    state: &tauri::State<'_, App>,
    table: &mut crate::watch::process::ProcessTable,
) {
    match reveal_plan(id, raised, host, editor, ide_dir, state, table) {
        Ok((cli, uri)) => {
            // **Asked once more, here.** The check inside the plan is worth having early, but
            // reading the locks, the port table and the editor's own path all happen after
            // it — and the editor hands a URL to whichever window is *then* focused. This is
            // the last instruction before the send, so it is the one that has to be true.
            if !raised.window.is_some_and(crate::win::foreground_is) {
                eprintln!("mcv-board: reveal skipped — the foreground moved before it was sent");
                return;
            }
            let Some(command) = launcher_command(&cli, &uri) else {
                eprintln!(
                    "mcv-board: reveal skipped — {} cannot be quoted safely",
                    cli.display()
                );
                return;
            };
            // **The editor's own launcher, not the shell's URL association.** The association
            // runs `Code.exe --open-url`, which that executable rejects outright — every URL
            // the board "sent" that way was discarded in silence. `bin/*.cmd` is what sets
            // `ELECTRON_RUN_AS_NODE` and hands the arguments to `cli.js` (ADR-0029).
            let sent = std::process::Command::new("cmd")
                // One pre-quoted string, not separate arguments. `cmd` re-parses everything
                // after `/c` by its own rules rather than as argv, so Rust's escaping does
                // not apply and the quoting has to be built for `cmd` itself: `/s` with the
                // whole thing wrapped in one more pair of quotes is the documented form that
                // strips only the outer pair. `/d` skips the `AutoRun` command a user or
                // anything else may have registered — this process should run the launcher
                // and nothing else.
                .raw_arg(command)
                // ...and without a console. `cmd` would otherwise show one, and hand it down
                // to the launcher, for the several seconds `cli.js` takes to answer.
                .creation_flags(crate::win::NO_CONSOLE)
                .spawn();
            match sent {
                Ok(_) => eprintln!("mcv-board: reveal sent {uri}"),
                Err(e) => eprintln!("mcv-board: reveal could not be sent ({e})"),
            }
        }
        // Printed rather than swallowed. Every arm here is a *correct* reason to do nothing,
        // and the trouble with correct silence is that it is indistinguishable from a broken
        // feature — which is the same argument `--check-raise` exists for.
        Err(why) => eprintln!("mcv-board: reveal skipped — {why}"),
    }
}

/// What to run and what to send, or the reason there is nothing to.
///
/// Each `Err` is a condition of ADR-0029 failing, and each is a reason the click should do
/// only what it always did: bring the window forward, and nothing else.
#[cfg(windows)]
fn reveal_plan(
    id: &str,
    raised: &crate::raise::Raise,
    host: u32,
    editor: u32,
    ide_dir: &std::path::Path,
    state: &tauri::State<'_, App>,
    table: &mut crate::watch::process::ProcessTable,
) -> Result<(std::path::PathBuf, String), String> {
    if !state
        .settings
        .lock()
        .is_ok_and(|settings| settings.reveal_session)
    {
        return Err("not switched on".to_owned());
    }
    if raised.outcome != Outcome::Raised {
        return Err(format!("the window was not raised ({:?})", raised.outcome));
    }
    let Some(window) = raised.window else {
        return Err("no window was found".to_owned());
    };
    // Asked again here rather than trusted from the raise: a quarter of a second passed while
    // that was verified, and anything at all may have taken the foreground since. The editor
    // hands a URL to whichever of its windows is active, so this is the only lever there is.
    if !crate::win::foreground_is(window) {
        return Err("something else took the foreground before it could be sent".to_owned());
    }

    let locks = crate::raise::read_locks(ide_dir);
    let ports = crate::win::listening_ports_of(host);
    let Some(lock) = crate::raise::lock_for(&locks, &ports) else {
        return Err(format!("no lock among ports {ports:?} claims this window"));
    };

    // **The documented precondition.** The handler resumes a session only where the session
    // belongs to the window's workspace; where it does not, it starts a *fresh* conversation
    // instead — so sending anyway would open a second copy of the session in the wrong place.
    let Some(cwd) = workspace_of(state, id) else {
        return Err(format!(
            "the board does not know where session {id} is running"
        ));
    };
    if !crate::raise::belongs_to_workspace(&cwd, &lock.workspace_folders) {
        return Err(format!(
            "{cwd} is not in this window's workspace {:?}, so the editor would start a fresh \
             conversation rather than resume this one",
            lock.workspace_folders
        ));
    }

    if lock.ide_name.is_empty() {
        return Err("the lock does not name the editor, so its URL scheme is unknown".to_owned());
    }
    let uri = crate::raise::session_uri(&lock.ide_name, id).ok_or_else(|| {
        format!(
            "{:?} is not an editor this build can address, or {id:?} is not a session id",
            lock.ide_name
        )
    })?;

    let Some(exe) = table.exe_of(editor) else {
        return Err(format!(
            "the editor process {editor} has no executable path"
        ));
    };
    let cli = crate::raise::editor_cli(&exe)
        .ok_or_else(|| format!("no single command-line launcher beside {}", exe.display()))?;
    Ok((cli, uri))
}

/// The command line for `cmd`, quoted for `cmd` rather than for a program.
///
/// `None` if either part contains a quote of its own, because there is then no way to say
/// where the argument ends and the rules stop being ours. Neither can in practice — a
/// Windows path cannot contain `"`, and the URL is built from an id checked to be
/// `[A-Za-z0-9-]` — so this is the belt on top of the braces, for the day one of those
/// stops being true.
#[cfg(windows)]
fn launcher_command(cli: &std::path::Path, uri: &str) -> Option<std::ffi::OsString> {
    let path = cli.to_str()?;
    if path.contains('"') || uri.contains('"') {
        return None;
    }
    Some(std::ffi::OsString::from(format!(
        r#"/d /s /c ""{path}" --open-url "{uri}"""#
    )))
}

/// Which workspace a session is running in, as the board already groups it.
///
/// The board's group key *is* the `cwd` from the session registry, so this asks the picture
/// on screen rather than reading the registry a second time.
///
/// **Two snapshots, and it does not matter.** The poll publishes `state.sessions` and
/// `state.board` separately, so a click can land between them — but a session's `cwd` is
/// fixed when it starts, so the only disagreement possible is a session present in one and
/// absent from the other. That answers `None`, and `None` sends nothing.
#[cfg(windows)]
fn workspace_of(state: &tauri::State<'_, App>, id: &str) -> Option<String> {
    let board = state.board.lock().ok()?;
    board
        .groups
        .iter()
        .find(|group| group.sessions.iter().any(|session| session.id == id))
        .map(|group| group.key.clone())
}

#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
fn reveal_session(
    _id: &str,
    _raised: &crate::raise::Raise,
    _host: u32,
    _editor: u32,
    _ide_dir: &std::path::Path,
    _state: &tauri::State<'_, App>,
    _table: &mut crate::watch::process::ProcessTable,
) {
}

/// Looks again, every [`POLL`], and tells the front end when something changed.
///
/// On its own thread rather than a timer in the window, so that reading files and probing
/// processes never sits between the user and a repaint.
fn start_watching(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut tick: u32 = 0;
        loop {
            std::thread::sleep(POLL);

            // Every eighth poll, about six seconds. A display change is not something the user
            // does twice a minute, and asking the window system where its monitors are is a
            // round trip through the event loop — four a second, forever, to answer a question
            // whose answer almost never changes is the cost NFR-03 is about.
            tick = tick.wrapping_add(1);
            if tick % 8 == 0 {
                rescue_if_lost(&app);
            }

            let Some(state) = app.try_state::<App>() else {
                return;
            };
            let Some(watcher) = state.watcher.as_ref() else {
                return;
            };

            let folded = state
                .settings
                .lock()
                .map(|settings| settings.folded.clone())
                .unwrap_or_default();

            let Ok(mut watcher) = watcher.lock() else {
                return;
            };
            let mut board = watcher.poll(now_millis(), &|key| folded.contains(key));
            let published = watcher.session_processes();
            drop(watcher);
            board.scale = state.settings.lock().map_or(100, |s| s.scale());

            if let Ok(mut sessions) = state.sessions.lock() {
                *sessions = published;
            }
            // The measurement survives the rebuild. Without this the poll pushed the window
            // back to 300 every 750 ms, and the renderer — which reports only when the set of
            // workspaces changes — never said it again.
            board.folded_width = folded_width_for(&state, &board);

            // Repaint only when something actually differs. A board that redraws every
            // 750 ms whether or not anything moved is a board that costs more than the
            // sessions it watches (NFR-03).
            let Ok(drawn) = serde_json::to_string(&board) else {
                continue;
            };
            // The roll-up and the mode come out of the same lock as the comparison: they are
            // what the tray shows, and reading them again afterwards would read a board that
            // had already been replaced.
            let Some((changed, was, mode)) = state.board.lock().ok().map(|held| {
                let changed = serde_json::to_string(&*held).is_ok_and(|previous| previous != drawn);
                (changed, held.rollup, held.mode)
            }) else {
                continue;
            };
            if !changed {
                continue;
            }

            if let Some(window) = app.get_webview_window("board") {
                fit(&window, board.width(), board.height());
            }
            if let Ok(mut held) = state.board.lock() {
                *held = board.clone();
            }
            // The tray is the board while the board is hidden (FR-32), so it follows the same
            // roll-up. Redrawn only when that roll-up moved: the poll runs four times a
            // second and the answer changes a handful of times an hour.
            if board.rollup != was {
                crate::tray::refresh(&app, board.rollup, state.theme);
            }
            // The tray's menu cannot be rebuilt when it opens — the shell opens it without
            // asking us — so it is replaced when the statement on it stops being true.
            if board.mode != mode {
                crate::tray::refresh_menu(&app, &state);
            }
            let _ = app.emit("board", board);
        }
    });
}

/// The renderer reporting how wide a fully folded board needs to be (§2.5).
///
/// Measured there because the font is proportional and §2 is written in pixels: "the longest
/// workspace name plus the chrome around it" is not a number this side can compute. The
/// clamp to [140, 300] and the decision to have two widths rather than a continuous fit stay
/// here, in [`BoardView::width`].
///
/// Recomputed only when the **set of workspaces** changes, never on a status or title change
/// — a board that re-measured itself whenever a title moved would twitch in the corner of the
/// eye all day ([ADR-0015](../../../docs/adr/0015-the-folded-board-narrows-to-its-names.md)).
#[tauri::command]
fn report_folded_width(width: u32, app: tauri::AppHandle, state: tauri::State<'_, App>) {
    let sized = {
        let Ok(mut board) = state.board.lock() else {
            return;
        };
        if !board.fully_folded || board.folded_width == Some(width) {
            return;
        }
        board.folded_width = Some(width);
        let signature = board.workspace_signature();
        if let Ok(mut remembered) = state.folded_width.lock() {
            *remembered = Some((signature, width));
        }
        (board.width(), board.height())
    };

    if let Some(window) = app.get_webview_window("board") {
        fit(&window, sized.0, sized.1);
    }
}
