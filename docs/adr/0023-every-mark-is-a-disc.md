# ADR-0023: Every indicator is a circle

- **Status**: Accepted; its silhouette table is superseded by [ADR-0024](0024-colour-carries-the-status-shape-carries-the-hierarchy.md), which took the status meanings off the shapes. The decision recorded here — that every mark is a circle — stands.
- **Date**: 2026-08-27
- **Deciders**: maintainer

## Context

The indicators were rounded squares: a 10 px chip with a 3 px radius, doubled to 20 px with a
6 px radius when the marks grew (§2). The shape was never argued for — it came from the first
sketch and survived because nothing challenged it.

Two things challenged it at once. The owner asked for circles, having looked at the board for
a day. And `awaiting_user` had just become a disc so that its wave would read as a ripple
rather than as an expanding rounded square — which left one round mark among five square ones
for no reason a user could infer.

The question this raises is not aesthetic. **FR-17 rests on the silhouettes being distinct**:
colour may be absent — greyscale, colour-vision deficiency, a small screenshot — and the
non-colour channels are the shape and the motion. Changing every shape at once is changing the
alphabet that requirement is written in.

## Decision

**Every mark is a circle of the cell's diameter.** What separates the six is what is drawn
inside that circle, not the outline of the box:

| Status | Mark |
|---|---|
| `working` | a filled disc |
| `awaiting_user` | a filled disc **plus a second ring**, the wave |
| `idle` | a hollow ring |
| `terminated` | a disc with its top-right **cut off by a straight edge** — the only mark that is not a whole shape |
| `limited` | a ring with an interior **level** |
| `unknown` | a **dotted** ring |

Two figures moved to keep that alphabet legible once the outline was round, and both were
found by looking at the result rather than by reasoning about it:

- **`terminated`'s cut went from 58/42 to 38/62.** A square has a corner to take; a circle
  offers a sliver at the same numbers, and at 20 px that sliver vanished. The mark collapsed
  into a plain disc — which is `working`. Of every confusion this set can produce, *leave it
  alone* against *this stopped and nobody asked it to* is the one that costs the most.
- **`limited`'s wall went from 1.6 to 1.2**, because a thicker wall left an interior too small
  for the level inside it to be seen.

## Alternatives considered

| Option | Good | Why not |
|---|---|---|
| Keep rounded squares | Nothing to change; the silhouettes were already checked | The owner's call, and the wave had already made one mark round. One circle among five squares is a difference the user cannot attribute to anything |
| Circles for the statuses that summon, squares for the rest | The shape itself would carry urgency | It gives the *outline* a meaning as well as the fill, so a user has two alphabets to learn instead of one. And a folded group's roll-up would change shape as its members changed status |
| Keep the square silhouettes and round only the corners further | Cheapest | A superellipse is neither one thing nor the other, and it does not fix the reason the change was asked for |

## Consequences

**Better**

- One shape language. Every mark is a circle, and every difference between them is a
  difference in what fills it.
- `terminated` is more legible than it was: the cut had to be re-tuned to survive the round
  outline, and the version that survives is more obviously a damaged shape than the old corner
  nick was.
- The ink figures all fell by π/4 — a disc is that fraction of the square that held it — and
  the ordering survived at both ends, which is all
  [ADR-0014](0014-the-ladder-pins-the-ends-not-the-middle.md) pins. In the middle,
  `terminated` and `limited` swapped, because a *level* loses more area to a circle than a
  solid disc does. That is precisely the part of the ordering ADR-0014 refuses to promise.

**Worse, or newly owed**

- **`limited` does not fully read as a level in a vessel at 20 px.** With the wall thinned it
  is a ring with a filled lower part and a dark crescent above — recognisable once known,
  weaker than intended, and closest in greyscale to the other partly-filled marks. It is the
  rarest status and its row also shows the reset time as text (TC-62), so the mark is not the
  only evidence; but the silhouette is carrying less than the specification claims for it, and
  that is a real gap rather than a matter of taste. Revisit if the indicator ever grows again.
- An **open** group header is drawn hollow (§2.4), which gives it the same silhouette as
  `idle`. They are separated by lightness and by everything else about the row. Named here
  because it is the one place the "no two silhouettes alike" rule is bent, and it is bent for a
  mark that is a summary rather than a status.
- The committed design images and the generator that draws them changed with the code. They
  are checked in, so a future reader sees circles rather than the squares this ADR replaced.
