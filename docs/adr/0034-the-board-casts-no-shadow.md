# ADR-0034: The board casts no shadow

- **Status**: Accepted
- **Date**: 2026-09-06
- **Deciders**: maintainer

## Context

[ADR-0022](0022-the-window-is-larger-than-the-board.md) made the window 16 px larger than the
panel on every side and turned the platform's drop shadow off, so that the two things the
board draws outside itself — the wave off an `awaiting_user` indicator, and a shadow — had
somewhere to go. The shadow became the panel's own `box-shadow`: two dark layers, 26 px and
6 px.

In use it is the wrong thing to be drawing.

- **A shadow on a transparent window is not chrome, it is content.** With decorations there is
  a frame and the shadow reads as the edge of a window. Here there is no frame: what is on
  screen is the panel, and then a soft grey rectangle standing off it in the transparent
  margin, extending past every edge the user thinks the window has.
- **On a light editor it is the most visible thing about the board.** The light theme's whole
  separation strategy is one darker border line, tuned to sit just clear of a white background
  ([§2.6](../ui-overlay.md), `Palette::edge`). A black halo at 50 % alpha over white is far
  louder than that line, so the first thing the eye finds is the smear around the board rather
  than the board — and louder than the status marks, which are the only things here that are
  supposed to compete for attention (FR-16).
- **It was buying a depth cue the board does not need.** A shadow says *this floats above
  that*. The board is `alwaysOnTop` and small; it is never in a stack the user has to read.
  The question it has to answer is "which session needs me", and nothing in that is helped by
  the panel appearing to hover.

## Decision

**Nothing about the board is drawn as a shadow.** The platform's is off (it already was) and
the panel's own `box-shadow` is removed. The separation from whatever is behind the board is
the border of §2.6 and nothing else.

The window stays 16 px larger than the panel. That margin is the wave's, and always was: the
wave reaches about three times the disc, which is what sets the size.

The one pulse of the edge that answers "where is it?" (`#panel.flash`) keeps its ring, and its
keyframes now start and end at a ring of zero width rather than at the resting drop shadow.

## Alternatives considered

| Option | Good | Why not |
|---|---|---|
| Keep it, lighter and tighter | The panel still lifts off the background; one number to tune | Any shadow visible enough to lift the panel is visible enough to be the halo the user objected to. Tuning it is choosing how much of the wrong thing to draw |
| Shadow on the dark theme only, none on the light | Keeps the lift where it is least offensive, drops it where it hurts | Then the board is a different object on the two themes, and the light theme — where separation is hardest — is the one left with less. The border was made to carry this on its own; let it |
| Shrink the margin now that the shadow is gone | A smaller transparent band takes fewer clicks that belong to the editor (ADR-0022's first cost) | The margin's size was never the shadow's to set. Shrinking it clips the wave, which is the thing ADR-0022 exists to protect |
| Leave it | Nothing to change; the shadow is conventional | The complaint is real and reproducible, and it is worst on the theme where the board is least visible to begin with |

## Consequences

**Better**

- The board ends where the user thinks it ends: the border is the outline, and there is
  nothing between it and the editor underneath.
- The light theme stops drawing a dark halo on a white background, which was the loudest
  unintended mark on the board.
- One less thing that has to look right against an unknown background. The border is measured
  against the palette (TC-48 … TC-51); a shadow is measured against whatever happens to be
  behind the window.

**Worse, or newly owed**

- **The board no longer reads as floating over the editor** — it reads as laid on it. The
  rounded corners and the lit top edge are what is left saying "panel", and on a background
  close to the board's own fill the border is doing all the work alone.
- **The transparent margin now has one tenant.** A 16 px band around the board still takes
  clicks that would reach the editor (ADR-0022), and the wave alone now justifies it. If the
  wave is ever redesigned to stay inside the panel, the margin should be revisited in the same
  change rather than surviving out of habit.

**What would reopen this**

- A board that has to be legible over a background it cannot predict — a photograph, a
  video call, a shared screen — where an edge line of any single colour can vanish. That is a
  case for a shaped outline first, and only then for a shadow.
