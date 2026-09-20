# ADR-0022: The window is larger than the board it draws

- **Status**: Accepted; its shadow is superseded by [ADR-0034](0034-the-board-casts-no-shadow.md), which took the shadow off the panel. The decision recorded here — that the window is larger than the board, and why — stands: the wave is what the margin is for.
- **Date**: 2026-08-27
- **Deciders**: maintainer

## Context

`awaiting_user` is the one status that means *stop what you are doing and go there*. Its
indicator carries a halo that expands and fades — the loudest thing the board is allowed to
draw ([ADR-0014](0014-the-ladder-pins-the-ends-not-the-middle.md)).

Two things pushed against it:

- **The halo could only ever grow to about 1.6× the mark.** Beyond that it hit the edge of the
  window and was cut off, because a window clips what it draws. A summons that stops at an
  invisible line a few pixels out is a summons the eye reads as a small nervous flicker.
- **The window had just gained a real edge** ([§2.6](../ui-overlay.md)) and, with it, the
  platform's drop shadow. A shadow is also something drawn *outside* the panel; it was there
  by the window manager's grace rather than by design, and it could not be shaped.

Both are the same problem: everything the board wants to draw outside itself has nowhere to go.

## Decision

**The window is 16 px larger than the panel on every side, and that space is transparent.**
The panel — border, title strip, rows — is drawn inside the margin; the shadow and the wave
off an `awaiting_user` indicator are drawn in it.

`transparent: true` on the window, the platform shadow off, and the panel carries its own
`box-shadow`. The panel deliberately does **not** clip its overflow: clipping it at the panel
edge is the exact thing this decision exists to stop, so the title strip rounds its own top
corners instead of the panel clipping them.

## Alternatives considered

| Option | Good | Why not |
|---|---|---|
| Leave the halo clipped at ~1.6× | Nothing to build; the window stays simple | The wave is the loudest signal in the product and it was being cut off mid-travel. It read as a flicker rather than as a call |
| Make the window as big as the largest thing ever drawn, opaque | No transparency, no hit-test question | The board would be a rectangle of opaque background much larger than the board, sitting on top of the editor. It would hide work to make room for a wave that is absent 99 % of the time |
| Draw the wave *inside* the panel, growing into the row | No window change at all | A row is 30 px and the wave needs three times the mark. It would run under the session title, which is the text the board exists to show |
| A per-pixel hit test, so the transparent margin passes clicks through | Nothing under the board becomes unclickable | Real work, and it has to keep changing: the region that should take clicks is the panel *plus whatever the wave is covering this frame*. The margin is 16 px; the owner judged the cost not worth the mechanism |

## Consequences

**Better**

- The wave reaches about three times the disc and crosses the board's edge. The one status
  that means the developer has to move is the one thing allowed to leave the board.
- The shadow is the product's own rather than the window manager's, and it can be shaped —
  which also means it looks the same wherever the board runs.
- Rounded corners become possible, which is what makes the board read as a panel laid over the
  editor rather than a hole cut in it.

**Worse, or newly owed**

- **The window's hit area is the whole window.** A 16 px transparent band around the board
  takes clicks that would otherwise reach the editor underneath. Accepted deliberately, and
  the alternative is in the table above.
- **The remembered position is the window's, not the panel's**, so the board appears 16 px in
  from the coordinates that are stored. Nothing visible is wrong, and the clamp that keeps a
  restored board on screen (TC-68) still works on the window, which is the larger rectangle —
  so it errs towards keeping the board visible.
- **Two sizes now exist and code has to say which it means.** `width()`/`height()` are the
  window; `panel_width()`/`panel_height()` are the board. The [140, 300] clamp of §2.5 is
  about the panel, and one test asserts the relationship rather than every test carrying the
  margin.
- A transparent window is a different rendering path in the WebView, and a future bug that
  only appears there will be hard to recognise as such. Noting it here so that it is
  recognisable.
