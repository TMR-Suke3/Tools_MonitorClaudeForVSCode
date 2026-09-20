//! The two keys the board listens for while something else has focus.
//!
//! Both exist for the same problem, from opposite ends: a small always-on-top window on a desk
//! with three displays is easy to lose and occasionally in the way.
//!
//! * **Bring the board to me** moves it to the display the mouse is on, puts it in front and
//!   flashes its edge once. No amount of border answers "which screen is it on?" — that is a
//!   question about finding a window, and only the window can answer it.
//! * **Show or hide** takes it away and brings it back where it was.
//!
//! **Neither takes focus** (FR-30). The board is summoned, looked at, and left where it is;
//! it never becomes the thing the keyboard is talking to.
//!
//! Registration can fail — another application may already hold the key — and when it does the
//! menu says so rather than leaving a key that quietly does nothing. That is FR-52's argument
//! in a different corner of the product: never present something you are not delivering.
//!
//! **Which is why the keys can be changed and cleared** (FR-61). A shortcut this product picked
//! cannot be the last word: the machine it runs on was somebody's before it was ours, and a key
//! that is already spoken for is not a defect to be reported but a choice to be undone. Every
//! outcome here is *measured* — the key is offered to the operating system and what comes back
//! is what the menu says — because whether something else holds a key is not a thing that can
//! be worked out from the key.

use tauri::{Emitter, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::app::App;

/// What became of one shortcut.
///
/// Five outcomes, and the distance between them is the point. *Cleared* and *taken* are both
/// "this key does nothing", and telling a user they are the same thing is telling them their
/// own decision is a fault. *Duplicate* is the same again from the other side: it looks
/// exactly like *taken* to the operating system, and saying so would blame a machine for
/// something this product did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Bound {
    /// Registered, and listening.
    Live,
    /// Cleared by the user. Nothing is listening, and nothing is wrong.
    #[default]
    Off,
    /// Something else on this machine already holds it.
    Taken,
    /// Not a key combination this platform can be asked for.
    Unreadable,
    /// The board's *other* shortcut is this same key.
    ///
    /// Separate from [`Bound::Taken`] because the repair is different and so is the blame. A
    /// key the operating system refuses is somebody else's; two rows of the same window set to
    /// the same key is this product failing to notice, and telling the user another application
    /// had it would send them hunting through their own machine for a culprit that is us.
    Duplicate,
}

/// What became of both of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct Registered {
    pub summon: Bound,
    pub toggle: Bound,
}

/// How an accelerator reads before anything is asked of the operating system.
///
/// Pure, and separated from the registering for exactly that reason: the two failures that can
/// be decided by looking at the string — it is empty, it is not a key combination — are the two
/// that can be tested without a keyboard, a window or a machine that does not already hold the
/// key (TC-130, TC-132).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Blank: the user asked for no key at all.
    Cleared,
    /// Nothing this platform can parse.
    Unreadable,
    /// A key combination, which may still turn out to be somebody else's.
    Key,
}

/// How an accelerator reads.
///
/// **A bare key is not one this product will take.** `M`, `F5`, `` ` `` and `Escape` all parse
/// perfectly well — measured — and registering one of them globally takes that key away from
/// every program on the machine for as long as the board runs. The setup window will not
/// produce one, but the settings file is a file: it can be edited by hand, copied between
/// machines, or written by an older build. So the rule lives here, where the key is actually
/// offered to the operating system, rather than only in the window where it is chosen.
#[must_use]
pub fn read(accelerator: &str) -> Reading {
    if accelerator.trim().is_empty() {
        return Reading::Cleared;
    }
    match accelerator.parse::<Shortcut>() {
        Ok(key) if key.mods.is_empty() => Reading::Unreadable,
        Ok(_) => Reading::Key,
        Err(_) => Reading::Unreadable,
    }
}

/// Registers both shortcuts, and reports what became of each.
///
/// **Each key on its own.** This used to answer with one boolean for the pair, and the reason
/// it no longer does is that both of its failure modes lied. A key another application had
/// taken made the menu report *both* as not registered, including the one that worked; and an
/// unparsable accelerator returned before the other key had been offered at all, so a typo in
/// one setting silently disabled the other.
pub fn register(app: &tauri::AppHandle) -> Registered {
    let Some(state) = app.try_state::<App>() else {
        return Registered::default();
    };
    let (summon, toggle) = {
        let Ok(settings) = state.settings.lock() else {
            return Registered::default();
        };
        (
            settings.hotkeys.summon.clone(),
            settings.hotkeys.toggle.clone(),
        )
    };

    // Unregister first, so that changing a key does not leave the old one live. Failing here
    // is ordinary — on the first run there is nothing to remove.
    let _ = app.global_shortcut().unregister_all();

    // The same key in both rows. The second registration would fail and `bind` would call that
    // *taken*, which is a lie with a repair attached to it: the user would go looking through
    // their own machine for the application holding a key this product is holding itself.
    // Summon keeps it, because it is the one bound first and because a board you cannot summon
    // is worse than one you cannot hide.
    let clash = read(&summon) == Reading::Key && summon.trim() == toggle.trim();

    Registered {
        summon: bind(app, &summon, Which::Summon),
        toggle: if clash {
            Bound::Duplicate
        } else {
            bind(app, &toggle, Which::Toggle)
        },
    }
}

/// Which of the two a binding is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Summon,
    Toggle,
}

/// Offers one accelerator to the operating system and reports what came back.
fn bind(app: &tauri::AppHandle, accelerator: &str, which: Which) -> Bound {
    match read(accelerator) {
        Reading::Cleared => return Bound::Off,
        Reading::Unreadable => return Bound::Unreadable,
        Reading::Key => {}
    }
    let Ok(key) = accelerator.parse::<Shortcut>() else {
        return Bound::Unreadable;
    };

    // **Every handler hears every shortcut.** `on_shortcut` registers the key, but the closure
    // it takes is called for any registered shortcut that fires — so each one has to ask
    // whether the key that fired is its own. Without that, pressing either key ran both
    // handlers: the board was summoned and then immediately hidden by the toggle, which is
    // what the first real press of it did.
    let handle = app.clone();
    let mine = key;
    let done = app
        .global_shortcut()
        .on_shortcut(key, move |_, fired, event| {
            if pressed(&event) && *fired == mine {
                match which {
                    Which::Summon => summon_board(&handle),
                    Which::Toggle => toggle_board(&handle),
                }
            }
        });

    // Refused, and the only refusal that happens in practice is that somebody else has it.
    // Reported as measured rather than as diagnosed: the board asked, and this is the answer.
    if done.is_ok() {
        Bound::Live
    } else {
        Bound::Taken
    }
}

/// A shortcut fires on the press and again on the release; only the press is an instruction.
fn pressed(event: &tauri_plugin_global_shortcut::ShortcutEvent) -> bool {
    event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed
}

/// Moves the board to the display the mouse is on, shows it, and flashes its edge.
///
/// The flash is the point as much as the move: after a keypress the user is looking somewhere
/// else on a large screen, and a window that has silently arrived is a window they still have
/// to find. One flash of the edge is the smallest thing that says *here*.
///
/// **The same corner every time, whoever asked.** The shortcut, the tray icon and a second
/// launch all end up here, and one rule is what makes "where will it appear?" a thing that can
/// be learned once
/// ([ADR-0031](../../../docs/adr/0031-one-corner-for-every-summons.md)).
pub fn summon_board(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("board") else {
        return;
    };

    // The display the pointer is on, or the primary one if the pointer is somewhere no monitor
    // claims — which happens between displays of different heights.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|cursor| window.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        // No monitor to be summoned to. Showing it where it already is beats leaving a
        // keypress unanswered.
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        return;
    };

    // The size the board will have **there**, not the one it has here. Work areas are physical
    // pixels, and a board measured on a 100 % display is 75 % narrower than the same board on a
    // 175 % one — so a corner worked out from the size it happens to have now puts a board
    // summoned across that boundary through the right-hand edge of the screen. The renderer's
    // own measurement is in CSS px, which is the one number that means the same thing on both.
    let size = css_size(app).map_or_else(
        || crate::app::window_size(&window),
        |(width, height)| {
            let scale = monitor.scale_factor();
            (
                (f64::from(width) * scale).round() as u32,
                (f64::from(height) * scale).round() as u32,
            )
        },
    );
    let (x, y) = crate::place::summon_position(crate::app::work_area(&monitor), size);

    let _ = window.show();
    let _ = window.set_position(PhysicalPosition::new(x, y));
    // Above everything, and still not focused: `set_focus` is the one thing this must not do
    // (FR-30). The board is always-on-top already, so showing it is enough to make it visible.
    let _ = window.set_always_on_top(true);
    let _ = app.emit("flash", ());
    // The tray menu states where the board is, and it has just moved.
    crate::tray::refresh_menu_from(app);
}

/// Hides the board, or brings it back where it was.
pub fn toggle_board(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("board") else {
        return;
    };
    if window.is_visible().unwrap_or(true) {
        let _ = window.hide();
        crate::tray::refresh_menu_from(app);
    } else {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        crate::tray::refresh_menu_from(app);
    }
}

/// The board's size as the renderer measured it, in CSS pixels.
///
/// `None` before the state exists, and if the lock is poisoned. The caller then falls back to
/// the window's own physical size, which is the pre-2026-09-20 behaviour and wrong across a
/// scale-factor boundary — but a board summoned to roughly the right corner beats a keypress
/// that does nothing, and both cases mean something has already gone wrong elsewhere.
fn css_size(app: &tauri::AppHandle) -> Option<(u32, u32)> {
    let state = app.try_state::<crate::app::App>()?;
    let board = state.board.lock().ok()?;
    Some((board.width(), board.height()))
}
