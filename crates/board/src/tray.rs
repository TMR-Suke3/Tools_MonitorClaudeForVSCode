//! The tray icon: the board when the board is not there (FR-32).
//!
//! It exists because of a hole this product dug for itself. `Ctrl+Alt+B` hides the board
//! (`docs/ui-overlay.md` §6.3), and until now the only way back was to remember the key. A
//! resident tool whose window can be dismissed needs somewhere it still lives, and on Windows
//! that place is the tray.
//!
//! Three things it does, and only the third needed a decision:
//!
//! 1. **It carries the board's own menu.** `menu::build` is called with the same state the
//!    right-click uses, which is why that menu was made native in the first place (§6.2) —
//!    two implementations of one menu is one too many.
//! 2. **Left-click brings the board here, or takes it away.** Nearly what a Windows tray icon
//!    does, and the difference is the whole of [`clicked`]: a plain show / hide leaves a board
//!    that is on a display which no longer exists exactly as invisible as it was.
//! 3. **It is drawn in the roll-up colour**, so a hidden board still answers "is anyone
//!    waiting for me?".
//!
//! ## Why the icon is a picture of the board rather than a coloured dot
//!
//! The palette's separations are measured against **the board's background** (§3.3): every
//! pair of indicator colours is checked for contrast and for distinguishability there, and
//! nowhere else. A bare disc on a transparent icon would sit on the taskbar instead, whose
//! colour is the user's to choose and against which nobody has measured anything.
//!
//! So the icon is the board in miniature: the panel's own fill, the panel's own edge, and the
//! mark in the middle. The pair of colours actually on screen is then the pair §3.3
//! validated, with no new measurement to make and none to skip. The edge does the second job
//! of separating the tile from whatever the taskbar is
//! ([ADR-0025](../../../docs/adr/0025-the-tray-icon-is-the-board-in-miniature.md)).

use mcv_core::color::Rgb;
use mcv_core::palette::{Silhouette, Status, Theme, indicator, rendering};
use tauri::Manager;
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::app::App;

/// The id the running icon is found by, when the roll-up changes and it has to be redrawn.
pub const ID: &str = "board";

/// The icon's side, in pixels.
///
/// Drawn larger than it is shown. Windows asks for 16 px at 100 % scaling and for 32 at
/// 200 %, and takes one bitmap for both; giving it the larger one makes the common case a
/// downscale, which softens the edges, rather than an upscale, which loses them.
pub const SIZE: u32 = 32;

/// Samples per pixel per axis, so an edge is a gradient rather than a staircase.
///
/// Sixteen samples a pixel over a 32 px square is 16 384 point tests, recomputed only when
/// the roll-up changes — a handful of times an hour, not once a frame.
const SUPERSAMPLE: u32 = 4;

/// The panel's corner, in proportion to the 4 px radius of a 300 px board (§2.7).
const CORNER_RADIUS: f64 = 3.0;
/// The outline, thick enough to survive the downscale to 16 px.
const EDGE_WIDTH: f64 = 2.0;
/// The mark. Larger relative to its tile than the board's 15 px disc is to its 300 px row:
/// this is read at 16 px across a taskbar, and a proportionate dot would be three pixels of
/// colour carrying the whole message.
const MARK_RADIUS: f64 = 8.0;
/// The hole in the ring drawn for `unknown`.
///
/// **A ring, where the board draws a dotted one** (§3.1). At the size this is actually shown
/// the dots are sub-pixel, and a dotted ring that resolves to a smudge reads as a filled disc
/// — which would say "a session is in some definite state" when the whole meaning of
/// `unknown` is that the board does not know. A ring says "nothing solid here" at any size.
const RING_INNER_RADIUS: f64 = 4.5;

/// Puts the icon in the tray, with the board's own menu on it.
///
/// # Errors
///
/// Anything the platform says about creating a tray icon. The caller keeps the board running
/// without one: a board with no tray is the board as it shipped yesterday, and one that
/// refused to start because the shell was busy would be worse than that.
pub fn build(app: &tauri::AppHandle, state: &App) -> tauri::Result<()> {
    let status = state
        .board
        .lock()
        .map_or(Status::Unknown, |board| board.rollup);
    let menu = crate::menu::build(app, state, crate::menu::Opened::OnTheTray)?;

    TrayIconBuilder::with_id(ID)
        .icon(icon(status, state.theme))
        .tooltip(tooltip(status))
        .menu(&menu)
        // The menu belongs to the right button. A left-click that opened a menu instead would
        // leave the hidden board with no one-click way back, which is the whole reason this
        // icon exists.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            // On the release, not the press: a click is not an instruction until the button
            // comes back up, and acting on the press means dragging the icon toggles the
            // board on the way past.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                clicked(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// A left-click on the icon: bring the board here, or take it away.
///
/// **Not a plain show / hide.** That is what this was until 2026-09-06, and it had a hole big
/// enough to lose the product in: showing a window whose position is on a display that no
/// longer exists shows nothing, so the one route back to a board that had gone missing was
/// itself a silent no-op. The click now decides from *where the board is* rather than from
/// whether it happens to be visible ([`crate::place::tray_click`]).
fn clicked(app: &tauri::AppHandle) {
    use crate::place::Click;

    let Some(window) = app.get_webview_window("board") else {
        return;
    };
    let visible = window.is_visible().unwrap_or(false);
    let reachable = window.outer_position().is_ok_and(|position| {
        crate::place::reachable(
            (position.x, position.y),
            crate::app::window_size(&window),
            &crate::app::work_areas(&window),
        )
    });

    match crate::place::tray_click(visible, reachable) {
        Click::Summon => crate::hotkeys::summon_board(app),
        Click::Hide => crate::hotkeys::toggle_board(app),
    }
}

/// Rebuilds the tray's menu from the state the board is in, for callers that hold only a
/// handle.
///
/// The menu states where the board is, and that sentence goes stale the moment the board is
/// moved, hidden or brought back — none of which happens anywhere near the state this needs.
pub fn refresh_menu_from(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<App>() {
        refresh_menu(app, &state);
    }
}

/// Redraws the icon for the status the board is now in.
///
/// Called from the poll, four times a second, and **only when the roll-up has changed** —
/// the caller compares, because it is the caller that already holds the previous board.
/// Handing the shell a new bitmap is cheap but not free, and an icon replaced 240 times a
/// minute for no reason is the kind of cost NFR-03 is about.
pub fn refresh(app: &tauri::AppHandle, status: Status, theme: Theme) {
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    let _ = tray.set_icon(Some(icon(status, theme)));
    let _ = tray.set_tooltip(Some(tooltip(status)));
}

/// Replaces the menu on the icon with one built for the board's current state.
///
/// The right-click menu is rebuilt every time it opens, because two of its items are
/// statements about that state (§6.2). The tray's menu cannot be: the shell owns it and
/// opens it without asking us. So it is replaced whenever one of those statements changes —
/// which is the mode, the size, and whether the shortcuts registered.
pub fn refresh_menu(app: &tauri::AppHandle, state: &App) {
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    if let Ok(menu) = crate::menu::build(app, state, crate::menu::Opened::OnTheTray) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// The icon for a status, in a theme.
#[must_use]
pub fn icon(status: Status, theme: Theme) -> Image<'static> {
    Image::new_owned(icon_pixels(status, theme), SIZE, SIZE)
}

/// The same pixels, before they become an [`Image`].
///
/// Separate so that `--tray-icons` prints **what the shell is given** rather than a second
/// drawing of it. An inspection command that renders the picture its own way can agree with
/// itself while disagreeing with the product, which makes it worse than having none.
#[must_use]
pub fn icon_pixels(status: Status, theme: Theme) -> Vec<u8> {
    let hollow = matches!(rendering(status).silhouette, Silhouette::DottedRing);
    pixels(
        theme.board(),
        theme.edge(),
        indicator(status, theme),
        hollow,
    )
}

/// What hovering the icon says.
///
/// The answer first and the product's name after it, because the answer is why anyone is
/// hovering. These are the roll-up's words, so each describes **the loudest thing on the
/// board** rather than every session on it — which is what the colour means too.
#[must_use]
pub fn tooltip(status: Status) -> String {
    let line = match status {
        Status::AwaitingUser => "A session is waiting for you",
        Status::Working => "Working — nothing is waiting for you",
        Status::Limited => "A session is held by the usage limit",
        Status::Terminated => "A session stopped abnormally",
        Status::Idle => "Nothing needs you",
        // The words §9 already uses for this on the board itself.
        Status::Unknown => "Not detecting anything",
    };
    format!("{line} — Claude session board")
}

/// The icon as straight (non-premultiplied) RGBA, row by row from the top left.
///
/// Kept apart from everything above so it can be checked without a tray, a shell or a
/// window: what this returns is the only part of the icon that can be wrong in a way anybody
/// would notice.
///
/// Three layers, composited in the order the board draws them: the edge, the panel inside it,
/// and the mark on top. `hollow` draws the mark as a ring rather than a disc — see
/// [`RING_INNER_RADIUS`].
#[must_use]
pub fn pixels(panel: Rgb, edge: Rgb, mark: Rgb, hollow: bool) -> Vec<u8> {
    let side = f64::from(SIZE);
    // A hair in from the icon's own bounds, so the outline is not clipped by them.
    let outer = RoundedRect {
        x0: 1.0,
        y0: 1.0,
        x1: side - 1.0,
        y1: side - 1.0,
        radius: CORNER_RADIUS,
    };
    let inner = outer.inset(EDGE_WIDTH);
    let centre = side / 2.0;

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut pixel = Pixel::CLEAR;
            pixel.over(edge, coverage(x, y, &|px, py| outer.contains(px, py)));
            pixel.over(panel, coverage(x, y, &|px, py| inner.contains(px, py)));
            pixel.over(
                mark,
                coverage(x, y, &|px, py| {
                    let distance = (px - centre).hypot(py - centre);
                    distance <= MARK_RADIUS && (!hollow || distance >= RING_INNER_RADIUS)
                }),
            );
            rgba.extend_from_slice(&pixel.rgba());
        }
    }
    rgba
}

/// What fraction of one pixel a shape covers, by sampling it on a grid.
fn coverage(x: u32, y: u32, inside: &dyn Fn(f64, f64) -> bool) -> f64 {
    let step = 1.0 / f64::from(SUPERSAMPLE);
    let mut hits = 0u32;
    for sy in 0..SUPERSAMPLE {
        for sx in 0..SUPERSAMPLE {
            let px = f64::from(x) + (f64::from(sx) + 0.5) * step;
            let py = f64::from(y) + (f64::from(sy) + 0.5) * step;
            if inside(px, py) {
                hits += 1;
            }
        }
    }
    f64::from(hits) / f64::from(SUPERSAMPLE * SUPERSAMPLE)
}

/// A rectangle with rounded corners, as a membership test.
struct RoundedRect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    radius: f64,
}

impl RoundedRect {
    /// The same rectangle, pulled in on every side. The corner tightens with it, so the
    /// outline lying between the two has a constant width all the way round.
    fn inset(&self, by: f64) -> Self {
        Self {
            x0: self.x0 + by,
            y0: self.y0 + by,
            x1: self.x1 - by,
            y1: self.y1 - by,
            radius: (self.radius - by).max(0.0),
        }
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
            return false;
        }
        // How far the point lies outside the rectangle the corner arcs are centred on. Zero
        // on both axes everywhere but the four corner squares, which is what makes this a
        // test of the corners alone.
        let dx = (self.x0 + self.radius - x)
            .max(x - (self.x1 - self.radius))
            .max(0.0);
        let dy = (self.y0 + self.radius - y)
            .max(y - (self.y1 - self.radius))
            .max(0.0);
        dx.hypot(dy) <= self.radius
    }
}

/// One pixel being built up, in straight alpha.
#[derive(Clone, Copy)]
struct Pixel {
    rgb: [f64; 3],
    alpha: f64,
}

impl Pixel {
    const CLEAR: Self = Self {
        rgb: [0.0; 3],
        alpha: 0.0,
    };

    /// Source-over compositing: `colour` at `alpha`, laid over what is already here.
    fn over(&mut self, colour: Rgb, alpha: f64) {
        if alpha <= 0.0 {
            return;
        }
        let out = alpha + self.alpha * (1.0 - alpha);
        for (channel, source) in self.rgb.iter_mut().zip(colour) {
            let above = f64::from(source) * alpha;
            let below = *channel * self.alpha * (1.0 - alpha);
            *channel = (above + below) / out;
        }
        self.alpha = out;
    }

    fn rgba(self) -> [u8; 4] {
        let byte = |v: f64| v.clamp(0.0, 255.0).round() as u8;
        [
            byte(self.rgb[0]),
            byte(self.rgb[1]),
            byte(self.rgb[2]),
            byte(self.alpha * 255.0),
        ]
    }
}
