# ADR-0024: Colour carries the status; the shape carries the hierarchy

- **Status**: Accepted
- **Date**: 2026-08-27
- **Deciders**: maintainer
- **Supersedes the silhouette half of** [ADR-0023](0023-every-mark-is-a-disc.md), whose
  decision — that every mark is a circle — stands and is unaffected.

## Context

FR-17 required a second, non-colour cue for every status and forbade any two statuses sharing
a silhouette. It was a good rule and it produced six shapes: a filled disc, a disc with a
wave, a hollow ring, a disc with a cut, a ring with a level in it, and a dotted ring.

Then the board was used. **Six shapes are six things to learn**, on an instrument that exists
to be glanced at — and the shapes were not carrying their weight even before that:

- **`limited`'s level did not read** at 20 px. That is measured, not felt: it was reviewed
  twice, the wall was thinned to give the level room, and it still looked like a filled disc
  with a notch. ADR-0023 recorded the failure honestly and left it.
- **`terminated`'s cut had to be re-tuned twice** for the same reason — 58/42 became 38/62
  because at the original figures the mark collapsed into `working`'s plain disc.
- The **hollow ring** was doing two jobs at once: it meant `idle`, and it meant *this is a
  workspace with its sessions shown below it*. Two meanings on one shape is the failure the
  rule was written to prevent, and the rule had produced it.

The question is what FR-17's second channel was actually insuring against, and the answer is
that the palette had already been derived to carry the status alone
([ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md)). Every pair keeps a stated
separation under greyscale and under simulated protanopia, deuteranopia and tritanopia, and
TC-49, TC-50 and TC-51 measure exactly that on every run. The second channel was insurance on
a number that is already checked.

## Decision

**Colour identifies the status. The shape says whether a mark is a workspace or a session.**

Every status is drawn as the same filled disc, with two exceptions that are not
identifications:

- **`unknown` keeps a dotted ring**, because §3.2 requires it to carry the least ink of the
  six — it is the observer admitting it cannot read a session, and a fallback must not read as
  an alarm. Filled, at 5.05:1, it would be the third loudest mark on the board.
- **An open workspace's own mark is hollow**, so that a parent and the sessions drawn beneath
  it are not read as a row of equal things. A folded workspace keeps the solid mark, because
  there it is the only evidence there is (FR-18).

`limited` loses its level and, with it, its motion: the level *was* the countdown, and the
row already shows the reset time as text. `terminated` loses its cut and keeps its blink.

FR-17 is amended rather than deleted, and the amendment states the loss plainly.

## Alternatives considered

| Option | Good | Why not |
|---|---|---|
| Keep all six silhouettes | Defence in depth; FR-17 unchanged | Six shapes to learn, two of which did not read at the size they are drawn. The redundancy was theoretical and the cost was on screen every day |
| Keep shapes only for the two that summon | Cheap; keeps a cue where confusion costs most | Those two are exactly the pair the palette separates most strictly (tier A, 15 dE2000 in every simulation). It keeps the cue where it is least needed and drops it where the greys are closest |
| Keep shapes, add a legend or tooltip glyph | Nothing lost | A board that needs a legend has failed at being glanceable. The glyphs already exist in the tooltip and are not the channel |
| Distinguish by size instead of shape | One dimension, easy to compare | Size is already spoken for: `awaiting_user` is the largest mark because it is the summons (TC-52c). A second meaning on the same dimension is the mistake this ADR is undoing |

## Consequences

**Better**

- One thing to learn: **the colour is the status**. Nothing else on the mark means anything
  except "this is a workspace" (hollow) and "this cannot be read" (dotted).
- The hollow ring now has exactly one meaning, which the view model decides rather than the
  stylesheet — it is a fact about the row, not about the session.
- Two shapes that did not survive their own review are gone rather than documented as
  known-weak.
- `limited` stops animating, so motion is now, without exception, "the developer has to move".

**Worse, or newly owed**

- **Defence in depth is gone.** A display, a screenshot pipeline, or a condition that mangles
  hue in a way the three simulations do not model now has nothing behind the palette. The
  three simulations are Machado 2009 at severity 1.0, which covers the common deficiencies and
  not every one.
- **TC-49 … TC-51 are now load-bearing.** They were belt and braces; they are the belt. A
  future palette change that fails them is no longer a degradation, it is a broken board.
- **The greyscale floor matters more than it did.** §3.3 sets 15 dE2000 for
  `awaiting_user` vs `terminated` and 4 for the rest; in greyscale every pair now has nothing
  but that number.
- `idle` is a filled disc rather than a hollow ring, so it carries more ink than it did. It
  is still the faintest of the five and still sits on the contrast floor
  ([ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md)), but the recession is now
  entirely the colour's doing.
