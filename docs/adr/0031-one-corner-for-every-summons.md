# ADR-0031 — One corner for every summons, and the board says where it is

- **Status**: Accepted
- **Date**: 2026-09-06
- **Requirement**: FR-28, FR-32
- **Refines**: [ADR-0025](0025-the-tray-icon-is-the-board-in-miniature.md), whose tray icon is
  the thing this makes work

## Context

A user reported the board missing. Not crashed — missing: *at some point the window is no
longer anywhere, and clicking the tray icon does not bring it back*.

Three separate things in this product added up to that.

1. **The check that keeps the board on a live display ran once, at startup.** FR-28 asks for a
   remembered position to be restored to a visible one, and that was read as a question about
   the *last* session. A display can be unplugged, put to sleep, or taken away by a remote
   session while the board is running, and Windows does not always move the windows that were
   on it. The board is frameless, has no taskbar button and is not in the Alt+Tab list
   (FR-26, FR-27), so a position outside every display is a window with no route back to it at
   all.
2. **The tray icon's left-click was a plain show / hide.** It never touched the position. So
   the one remaining route back — the icon that exists precisely because the board can be
   dismissed — did nothing visible in exactly the case where it was needed: showing a window
   that is positioned on a display which no longer exists shows nothing.
3. **Nothing told the user which of those was happening.** A hidden board, a board on a
   vanished display, and a board that is drawn nowhere despite being where it should be are
   the same picture: a screen with no board on it.

## Decision

**One placement rule, used by every summons.** The shortcut, the tray icon's click, the tray
menu's *Bring the board here*, and a second launch of the executable all put the board at the
**top-right of the work area of the display the pointer is on**.

**The rescue runs while the board runs**, on the poll that was already there, once every eight
ticks — about six seconds. Its test is *reachable*: at least a 48 × 24 patch of the board
overlaps some display's work area. Not *fully on one display*.

**The tray menu states where the board is** — hidden, off every display, or on display N — and
carries the action that fetches it.

## Consequences

**The corner moved.** It was the top-left, and only the shortcut used it. Two rules for one
question would have been worse than either rule: "where does the board appear?" is something a
user learns once, and an answer that depends on which of four things they pressed is not
learnable. The top-right is the one that agrees with where the summons comes from — the tray,
in the corner of the screen the taskbar is in — and it is the corner an editor's own content
is least often in.

**The work area, not the display's bounds.** A board placed at the corner of the bounds is a
board partly under the taskbar, and this one cannot be dragged out from under it by its title
bar, because it does not have one.

**Reachable rather than fully visible, and the negative cases are the design.** A rescue that
fires when it should not is worse than one that fires late: it is a board that walks away from
where its owner put it, every six seconds, for reasons they cannot see. Straddling two
displays and parking half over an edge are both things people do with a small always-on-top
window, and both stay untouched. The rescue only ever fires when there is not enough of the
board left to take hold of.

**Saying where it is costs one line and answers a question the product could not be asked.**
If the line names the display the user is looking at and there is still nothing there, then the
window is where it should be and what failed is the drawing — which is a different fault, in a
different layer, and one no amount of moving the window would have found.

**The arithmetic is a module with no platform in it** (`crates/board/src/place.rs`). Every case
here is otherwise reproducible only by unplugging a monitor, which is to say not reproducible
in a test at all; separated from the window system it is a position, a size and a list of
rectangles (TC-113 … TC-127). This is the same split NFR-10 asks for in the core, applied to
the one part of the board that has to be right on hardware nobody has.
