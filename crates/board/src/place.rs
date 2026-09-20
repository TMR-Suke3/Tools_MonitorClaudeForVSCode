//! Where the board goes, as arithmetic.
//!
//! Everything here is a pure function of a position, a size and a list of rectangles. Nothing
//! in this file asks the operating system anything, which is the only reason any of it can be
//! tested: the failure it exists to prevent — *the board is somewhere I cannot see* — is
//! otherwise reproducible only by unplugging a monitor.
//!
//! Three questions are answered, and they are the same question at three moments:
//!
//! 1. **Where does a summoned board go?** [`summon_position`] — the top-right of the display
//!    the user is looking at, inside its *work area* rather than its bounds, so it never
//!    lands under the taskbar.
//! 2. **Is the board still reachable?** [`reachable`], and [`rescue`] when it is not. A
//!    display can be unplugged, put to sleep, or taken away by a remote session while the
//!    board is sitting on it, and the position it was left at is then a place no mouse can
//!    go.
//! 3. **Where is it right now?** [`whereabouts`] and [`where_line`]. A board that cannot be
//!    seen has two very different explanations — it is hidden, or it is on screen somewhere
//!    nobody can look — and from the outside they are identical. The tray menu says which.

use crate::settings::{Area, clamp_to_displays};

/// The gap between the board and the corner it is summoned to.
///
/// Not flush: a board hard against the corner reads as part of the system chrome, and the
/// pixel row next to a screen edge is where the mouse ends up by accident.
pub const INSET: i32 = 40;

/// The smallest patch of the board that has to be visible for the board to count as reachable.
///
/// The body is the drag handle (FR-27), so "reachable" means *there is enough of it to take
/// hold of*, not *all of it is on screen*. The size is deliberately small. Rescuing a board
/// the user placed themselves would be the worse failure of the two: someone who parks it
/// half off the bottom edge to keep it out of the way has not lost it, and a board that crept
/// back every six seconds would be a board fighting its owner.
pub const GRAB: (i32, i32) = (48, 24);

/// Where a summoned board is placed on a given work area.
///
/// **The top-right**, and the same corner wherever the summons came from — the tray icon, the
/// shortcut, or a second launch. One rule, so that the answer to "where will it appear?" can
/// be learned once ([ADR-0031](../../../docs/adr/0031-one-corner-for-every-summons.md)).
///
/// A board larger than the work area is put at the corner rather than pushed off the opposite
/// edge: the inset is a courtesy, and being on screen is not.
#[must_use]
pub fn summon_position(work: Area, size: (u32, u32)) -> (i32, i32) {
    let (width, height) = (size.0 as i32, size.1 as i32);
    let x = (work.x + work.width as i32 - width - INSET).max(work.x);
    let y = (work.y + INSET).min(work.y + (work.height as i32 - height).max(0));
    (x, y)
}

/// Whether enough of the board overlaps some work area to be seen and taken hold of.
///
/// Measured against **each** rectangle rather than against their union, which is what keeps a
/// board straddling two displays reachable: each half on its own is larger than [`GRAB`].
#[must_use]
pub fn reachable(position: (i32, i32), size: (u32, u32), work_areas: &[Area]) -> bool {
    work_areas.iter().any(|area| {
        let (width, height) = overlap(position, size, area);
        width >= GRAB.0 && height >= GRAB.1
    })
}

/// Where an unreachable board should be moved to, or `None` if it is fine where it is.
///
/// *Nearest*, not *summoned*: this is the automatic case, and the user did not ask for
/// anything. Someone whose second monitor went to sleep wants the board back roughly where
/// they had it, not thrown to a corner of a screen they were not using — the same rule
/// FR-28 already states for the position remembered across a restart.
///
/// **[`GRAB`] decides whether to move the board, not where it lands.** Once this does act the
/// board is put *fully* inside the nearest work area, which is further than the threshold
/// strictly demands and is the point: a board nudged just far enough to show a 48 × 24 patch
/// would pass [`reachable`] while being useless, and the failure this exists to repair is *I
/// cannot see the board*, not *I cannot grab the board*. The threshold is deliberately hard
/// to trip precisely because crossing it moves the board all the way back.
#[must_use]
pub fn rescue(position: (i32, i32), size: (u32, u32), work_areas: &[Area]) -> Option<(i32, i32)> {
    if reachable(position, size, work_areas) {
        return None;
    }
    let moved = clamp_to_displays(position, size, work_areas)?;
    (moved != position).then_some(moved)
}

/// Where a board that a **resize** has just pushed off a display should be put back to.
///
/// `None` unless the board was wholly on some work area before the resize and is not after.
/// That condition is the whole of the rule, and it is what keeps this from arguing with
/// [`rescue`]'s restraint: a board its owner parked half over an edge did not get there by
/// growing, was not on a display to begin with, and is left exactly where it is.
///
/// The board grows right and down from its top-left, so a board summoned to the top-right
/// corner has only [`INSET`] to grow into before it is over the edge. Two ordinary things
/// spend that: a session appearing, which makes the board taller, and a display whose scale
/// factor is higher than the one the board was measured on, which makes it 75 % wider here.
#[must_use]
pub fn contain_growth(
    position: (i32, i32),
    was: (u32, u32),
    now: (u32, u32),
    work_areas: &[Area],
) -> Option<(i32, i32)> {
    let (x, y) = position;
    let held = work_areas.iter().find(|area| inside(position, was, area))?;
    // **Some** area, not the one found above. Work areas can overlap — a virtual display over
    // a real one is the ordinary way it happens — and a board that grew out of the first one
    // listed while staying wholly on the second has not gone anywhere the user cannot see.
    // Asking about `held` alone would make the platform's ordering visible as a board that
    // twitches on one desk and not another.
    if work_areas.iter().any(|area| inside(position, now, area)) {
        return None;
    }
    // Back by exactly the overhang, and never past the near edge: a board too large for the
    // display it is on is put at that display's corner rather than pushed off the other side.
    //
    // In `i64`. These are `u32` sizes arriving from outside, and a width past `i32::MAX` read
    // as a negative one would put the board somewhere no arithmetic here intends.
    let shift = |near: i32, span: u32, at: i32, size: u32| {
        let far = i64::from(near) + i64::from(span) - i64::from(size);
        i64::from(at).min(far).max(i64::from(near)) as i32
    };
    Some((
        shift(held.x, held.width, x, now.0),
        shift(held.y, held.height, y, now.1),
    ))
}

/// Whether a window of `size` at `position` is wholly within `area`.
///
/// In `i64` for the same reason as [`whereabouts`]: the sizes are `u32` and a display can sit
/// far from the origin, so the far edges are sums this cannot assume fit in an `i32`.
fn inside(position: (i32, i32), size: (u32, u32), area: &Area) -> bool {
    let (x, y) = (i64::from(position.0), i64::from(position.1));
    x >= i64::from(area.x)
        && y >= i64::from(area.y)
        && x + i64::from(size.0) <= i64::from(area.x) + i64::from(area.width)
        && y + i64::from(size.1) <= i64::from(area.y) + i64::from(area.height)
}

/// How much of a window of `size` at `position` lies inside `area`, as a width and a height.
fn overlap(position: (i32, i32), size: (u32, u32), area: &Area) -> (i32, i32) {
    let (x, y) = position;
    let (width, height) = (size.0 as i32, size.1 as i32);
    let left = x.max(area.x);
    let top = y.max(area.y);
    let right = (x + width).min(area.x + area.width as i32);
    let bottom = (y + height).min(area.y + area.height as i32);
    ((right - left).max(0), (bottom - top).max(0))
}

/// What a click on the tray icon means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Show the board and bring it to the display the pointer is on.
    Summon,
    /// Take it away.
    Hide,
}

/// A left-click on the tray icon, decided from what the board is doing.
///
/// The rule the user learns is *the icon brings the board to me, and the icon takes it away
/// again* — one click, and the answer never depends on remembering what state it was left in.
///
/// The middle row is the one this exists for. A board that is visible but on no display looks
/// exactly like a board that is hidden, and until 2026-09-06 the click did the same thing in
/// both cases: it toggled the window's visibility and never touched its position, so clicking
/// the icon "showed" a window that stayed exactly as invisible as it had been. That is a
/// no-op the user cannot tell from a dead tray icon.
#[must_use]
pub const fn tray_click(visible: bool, reachable: bool) -> Click {
    match (visible, reachable) {
        (true, true) => Click::Hide,
        _ => Click::Summon,
    }
}

/// Where the board is, in the three cases that are worth telling apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whereabouts {
    /// Not on screen at all, because something hid it.
    Hidden,
    /// On screen, at a position no display covers.
    Lost,
    /// On screen and reachable, on the display at this index.
    On { display: usize },
}

/// Which of the three the board is in.
#[must_use]
pub fn whereabouts(
    visible: bool,
    position: (i32, i32),
    size: (u32, u32),
    work_areas: &[Area],
) -> Whereabouts {
    if !visible {
        return Whereabouts::Hidden;
    }
    // The display showing the most of it, which is the one a user would name. In `i64`: two
    // sides of a large window multiply to more than an `i32` holds, and a board reported as
    // being on the wrong screen would be worse than no line at all.
    let best = work_areas
        .iter()
        .enumerate()
        .map(|(index, area)| {
            let (width, height) = overlap(position, size, area);
            (index, width, height)
        })
        .filter(|(_, width, height)| *width >= GRAB.0 && *height >= GRAB.1)
        .max_by_key(|(_, width, height)| i64::from(*width) * i64::from(*height));
    match best {
        Some((display, ..)) => Whereabouts::On { display },
        None => Whereabouts::Lost,
    }
}

/// The board's whereabouts as the sentence the tray menu shows.
///
/// This line is the whole diagnosis of "I cannot see the board". The three states have three
/// different repairs and no visible difference: *hidden* was dismissed and comes back with a
/// click, *lost* is a display that went away, and *on display N* means the window is where it
/// says it is — so if there is still nothing there, the thing that failed is the drawing, and
/// no amount of moving the window will fix it. A user cannot report which of those is
/// happening unless the product tells them.
#[must_use]
pub fn where_line(whereabouts: Whereabouts) -> String {
    match whereabouts {
        Whereabouts::Hidden => "The board is hidden".to_owned(),
        Whereabouts::Lost => "The board is off every display".to_owned(),
        Whereabouts::On { display } => format!("The board is on display {}", display + 1),
    }
}
