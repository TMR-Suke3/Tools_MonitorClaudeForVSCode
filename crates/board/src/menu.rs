//! The board's own menu (`docs/ui-overlay.md` §6).
//!
//! **Native, not drawn.** The board is a small always-on-top instrument and everything about
//! it is custom — the chrome, the marks, the wave — but a menu is not a place to be original:
//! it wants the platform's keyboard handling, its edge flipping, its screen-reader support
//! and its habits. It is also the same menu the tray icon will carry (FR-32), and two
//! implementations of one menu is one too many.
//!
//! What the menu is *for*, in the order the items appear:
//!
//! 1. **Say which mode the board is in.** FR-55 asks for this to be visible from the menu at
//!    all times; until now it has been in a tooltip and in the setup window, which is where
//!    it went while there was no menu to put it in.
//! 2. **Let the board be made bigger.** The design's own size is right for the author's
//!    display and not for everyone's (§2.8).
//! 3. **Say what the shortcuts are**, and say plainly when one of them is not registered —
//!    a shortcut another application has taken is a feature that silently does nothing,
//!    which is the failure FR-52 is about in a different corner of the product.
//! 4. **Open the diagnosis**, which is the rest of FR-55. The line at the top says *what* the
//!    board is doing; the diagnosis says which precondition decided that, and what single
//!    action changes it (`docs/hook-setup.md` §5).
//! 5. **Say where the board is**, on the tray's copy of the menu. The one question this
//!    product could not previously be asked is *where has the window gone*, and its three
//!    answers — hidden, on a display that no longer exists, and right where it should be —
//!    look identical from the outside and have three different repairs.

use tauri::Manager;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::app::App;
use crate::hotkeys::{Bound, Registered};
use crate::view::SCALES;

/// Ids the click handler matches on. Strings, because that is what the menu API carries.
pub const HOOK_SETUP: &str = "hook-setup";
pub const DIAGNOSE: &str = "diagnose";
pub const REVEAL: &str = "reveal";
pub const SUMMON: &str = "summon";
pub const SHORTCUTS: &str = "shortcuts";
pub const QUIT: &str = "quit";
/// `scale:125` and so on — the percentage is parsed back out of the id.
const SCALE_PREFIX: &str = "scale:";

/// Which of the two menus is being built.
///
/// One menu, two openers, and exactly one difference between them: the tray's copy says where
/// the board is and offers to fetch it. On the board's own right-click both are answered
/// already — a menu opened *by clicking the board* is a menu whose owner has found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// By right-clicking the board itself.
    OnTheBoard,
    /// By right-clicking the tray icon.
    OnTheTray,
}

/// Builds the menu for the state the board is in right now.
///
/// Rebuilt on every right-click rather than kept and mutated: it has to show the current mode
/// and the current size, and a menu built once would be a menu that is wrong by the second
/// time it is opened.
///
/// # Errors
///
/// Any failure from the menu API. The caller treats it as "no menu appears", which is bad but
/// not worth taking the board down for.
pub fn build(
    app: &tauri::AppHandle,
    state: &App,
    opened: Opened,
) -> tauri::Result<Menu<tauri::Wry>> {
    let mode = state.board.lock().map_or("inferred", |board| board.mode);
    let scale = state.settings.lock().map_or(100, |s| s.scale());

    // Disabled, because it is a statement rather than an action. FR-55 asks for the mode to
    // be visible from the menu at all times; it does not ask for it to be clickable.
    let heading = MenuItem::with_id(app, "mode", mode_line(mode), false, None::<&str>)?;

    let mut sizes: Vec<CheckMenuItem<tauri::Wry>> = Vec::new();
    for percent in SCALES {
        sizes.push(CheckMenuItem::with_id(
            app,
            format!("{SCALE_PREFIX}{percent}"),
            format!("{percent} %"),
            true,
            percent == scale,
            None::<&str>,
        )?);
    }
    let size_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = sizes
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let size = Submenu::with_items(app, "Size", true, &size_refs)?;

    let bound = state
        .hotkeys
        .lock()
        .map_or_else(|_| Registered::default(), |held| *held);
    let shortcuts = state.settings.lock().map_or_else(
        |_| Vec::new(),
        |settings| {
            vec![
                (
                    "Bring the board to me",
                    settings.hotkeys.summon.clone(),
                    bound.summon,
                ),
                (
                    "Show or hide",
                    settings.hotkeys.toggle.clone(),
                    bound.toggle,
                ),
            ]
        },
    );
    let mut shortcut_items: Vec<MenuItem<tauri::Wry>> = Vec::new();
    for (what, key, state_of_it) in shortcuts {
        shortcut_items.push(MenuItem::with_id(
            app,
            format!("hotkey:{what}"),
            shortcut_label(what, &key, state_of_it),
            false,
            None::<&str>,
        )?);
    }
    // The way out of a key that is somebody else's. It sits under the two lines it is about,
    // and it is the reason those lines can afford to be statements: the sentence says what is
    // wrong, and the item directly beneath it is what fixes it (FR-61).
    let rebind = MenuItem::with_id(app, SHORTCUTS, "Change these keys…", true, None::<&str>)?;

    // Named for what the user gets rather than for what the product does to get it. The
    // line at the top of this menu says waiting is "worked out from a pause"; this is the
    // item that stops it being worked out, and it has to say so on its own — three items
    // separate the two, so it cannot lean on the statement above.
    //
    // **Ticked when the mode is exact**, and the ellipsis stays. The tick elsewhere in this
    // menu means "on, and one click switches it off"; here it means "on" and the click
    // opens the removal screen instead, because taking the entries out rewrites the user's
    // own `settings.json` and FR-54 does not allow that without a decision. The ellipsis is
    // what says so: every item in this product that explains before it acts carries one.
    //
    // The distance from the mode line is why this is here at all. That line is a sentence,
    // and a sentence three items away is not what a reader scans for — the author of this
    // menu installed the hooks, went looking for a tick, and did not find one.
    let setup = CheckMenuItem::with_id(
        app,
        HOOK_SETUP,
        "Notice waiting instantly…",
        true,
        is_exact(mode),
        None::<&str>,
    )?;
    // The other half of FR-55, and a **separate item** from the line at the top on purpose:
    // that line is a statement, and this is the action. It is shown whatever the mode,
    // because "why is it exact" is as fair a question as "why is it not" — and because an
    // item that appeared only once something was broken would be an item nobody had ever
    // seen by the time they needed it.
    let diagnose = MenuItem::with_id(app, DIAGNOSE, "Check the reporting…", true, None::<&str>)?;

    // Two shapes for one entry, and the difference is the consent. **Off**, it is an item
    // ending in an ellipsis: it explains before anything happens, because turning it on lets
    // the board hand a URL to the editor and the editor decides which window answers
    // (ADR-0029). **On**, it is a tick that switches off in one click — undoing something the
    // user has already agreed to needs no second screen.
    let revealing = state
        .settings
        .lock()
        .is_ok_and(|settings| settings.reveal_session);
    let reveal_off = MenuItem::with_id(
        app,
        REVEAL,
        "Focus the session tab on click…",
        true,
        None::<&str>,
    )?;
    let reveal_on = CheckMenuItem::with_id(
        app,
        REVEAL,
        "Focus the session tab on click",
        true,
        true,
        None::<&str>,
    )?;
    // Where the board is, and the one action that brings it here — on the tray's copy only.
    //
    // This pair is the answer to a question the product could not previously be asked: *the
    // board is not there any more*. Hidden, on a display that has gone away, and drawn but
    // not visible are three different faults with three different repairs and no visible
    // difference between them, and the user cannot report which one they have unless
    // something says so. The line above the item is that something: if it names the display
    // they are looking at and there is still nothing there, the window is where it should be
    // and what failed is the drawing.
    let whereabouts = MenuItem::with_id(app, "whereabouts", where_line(app), false, None::<&str>)?;
    let summon = MenuItem::with_id(app, SUMMON, "Bring the board here", true, None::<&str>)?;
    let before_whereabouts = PredefinedMenuItem::separator(app)?;

    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    // Two, not one referenced twice. A menu item belongs to a menu at a position, and handing
    // the same one over at two positions is asking the platform to hold it in two places.
    let after_mode = PredefinedMenuItem::separator(app)?;
    let before_actions = PredefinedMenuItem::separator(app)?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        vec![&heading, &after_mode, &size];
    for item in &shortcut_items {
        items.push(item);
    }
    items.push(&rebind);
    if opened == Opened::OnTheTray {
        items.push(&before_whereabouts);
        items.push(&whereabouts);
        items.push(&summon);
    }
    items.push(&before_actions);
    if revealing {
        items.push(&reveal_on);
    } else {
        items.push(&reveal_off);
    }
    items.push(&diagnose);
    items.push(&setup);
    items.push(&quit);

    Menu::with_items(app, &items)
}

/// Whether the hook layer is doing the work right now.
///
/// **One place decides, two read it**: the statement at the top of the menu and the tick on
/// the item that turns it on. A menu that ticked the item while the line above said waiting
/// was worked out from a pause would be a menu contradicting itself, and the two are far
/// enough apart -- three items -- that nobody building either would notice.
///
/// `went-quiet` is not exact. Installed and silent reads the same to a user as never
/// installed: the board is guessing either way (FR-52), and the menu is not the place to
/// explain the difference -- *Check the reporting…* is.
#[must_use]
pub fn is_exact(mode: &str) -> bool {
    mode == "exact"
}

/// One shortcut's line in the menu: what it does, the key, and what became of it.
///
/// The key is shown whether or not it works, and the tail of the label says which. A shortcut
/// another application has already taken is the one case where saying nothing would be a lie
/// the user cannot see through: they press it, nothing happens, and the board looks broken
/// rather than out-voted.
///
/// **A key the user cleared is not a fault**, and does not read as one. That distinction is
/// the whole reason there are four outcomes rather than a boolean: *off* is a decision, and a
/// product that reported it as a failure would be arguing with its owner.
#[must_use]
pub fn shortcut_label(what: &str, key: &str, bound: Bound) -> String {
    match bound {
        Bound::Live => format!("{what}    {key}"),
        Bound::Off => format!("{what}    not set"),
        Bound::Taken => format!("{what}    {key} — another application has it"),
        Bound::Unreadable => format!("{what}    {key} — not a key this can use"),
        Bound::Duplicate => format!("{what}    {key} — the board's other shortcut has it"),
    }
}

/// Where the board is, as the sentence the tray menu shows.
///
/// The arithmetic is [`crate::place::whereabouts`], which is where it can be tested; this asks
/// the window system the three questions that feed it. A window the platform will not answer
/// for is reported as hidden — it is certainly not somewhere the user can see.
fn where_line(app: &tauri::AppHandle) -> String {
    let Some(window) = app.get_webview_window("board") else {
        return crate::place::where_line(crate::place::Whereabouts::Hidden);
    };
    let visible = window.is_visible().unwrap_or(false);
    // Not `(0, 0)` as a stand-in. The origin is a real place on someone's desk, and a window
    // the platform will not answer for would be reported as sitting on the first display —
    // the one confident answer this line must never give.
    let Ok(position) = window.outer_position() else {
        return crate::place::where_line(crate::place::Whereabouts::Hidden);
    };
    crate::place::where_line(crate::place::whereabouts(
        visible,
        (position.x, position.y),
        crate::app::window_size(&window),
        &crate::app::work_areas(&window),
    ))
}

/// The mode, in the words the rest of the product uses for it.
fn mode_line(mode: &str) -> String {
    let state = if is_exact(mode) {
        "exact"
    } else {
        "worked out from a pause"
    };
    format!("Waiting for you is {state}")
}

/// What a click on the menu does.
///
/// Returns whether the click was one of ours, so the caller can ignore anything else without
/// having to know what the menu contains.
pub fn clicked(app: &tauri::AppHandle, id: &str) -> bool {
    if let Some(percent) = id.strip_prefix(SCALE_PREFIX) {
        if let Ok(percent) = percent.parse::<u32>() {
            crate::app::set_scale(app, percent);
        }
        return true;
    }
    match id {
        HOOK_SETUP => {
            let _ = crate::setup::open_hook_setup(app.clone());
            true
        }
        DIAGNOSE => {
            let _ = crate::setup::open_hook_diagnosis(app.clone());
            true
        }
        REVEAL => {
            // On means the tick was clicked, which is the way *out*: no explanation is owed
            // for stopping something. Off means the ellipsis was clicked, and that explains
            // first — nothing is turned on from the menu alone.
            let on = app
                .try_state::<App>()
                .is_some_and(|state| state.settings.lock().is_ok_and(|s| s.reveal_session));
            if on {
                crate::app::set_reveal(app, false);
            } else {
                let _ = crate::setup::open_reveal_setup(app.clone());
            }
            true
        }
        SUMMON => {
            crate::hotkeys::summon_board(app);
            true
        }
        SHORTCUTS => {
            let _ = crate::setup::open_shortcuts_setup(app.clone());
            true
        }
        QUIT => {
            app.exit(0);
            true
        }
        _ => false,
    }
}
