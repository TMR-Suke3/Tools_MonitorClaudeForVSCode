# ADR-0010: Derive the palette from a luminance ladder, and set the thresholds by consequence

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author

## Context

The state model shipped a palette described as "provisional, chosen for separability", and
open decision **D-2** left the real one to the design pass. FR-16 makes colour the primary
status channel and FR-17 forbids colour being the *only* channel; `ui-overlay.md` §7 asked
for at least 3:1 contrast, legibility at 10 px, and survival under the common colour-vision
deficiencies.

"Chosen for separability" turned out to be the problem. Measuring the provisional palette
(CIEDE2000, with the Machado et al. 2009 simulation matrices at severity 1.0):

| Pair | View | dE2000 |
|---|---|---|
| `working` lime vs `awaiting_user` amber | deuteranopia | **6.2** |
| `terminated` red vs `limited` violet | greyscale | **1.5** |
| `unknown` vs the board background | — | **2.35:1** contrast, below the 3:1 the same document required |

The first of those is the worst possible place to fail: *leave it alone* against *go here*,
for the commonest colour-vision deficiency.

A hand-picked replacement failed too, differently — `working` and `awaiting_user` landed on
identical relative luminance, dE2000 **0.0** in greyscale. Picking hues first and hoping the
lightness works out does not converge.

## Decision

**Generate each colour from a target CIE L\* and a target Lab hue angle, taking the most
chroma that stays inside sRGB; fix the hue families by hand as a design decision; search
only the lightness; and judge the result against thresholds set per pair by consequence.**

Three constraints make the palette, in this order:

1. **A lightness ladder, with prominence following urgency.** `awaiting_user` is the
   brightest thing the board can draw and sits at least 12 L\* above everything else;
   `terminated` is deliberately dark so it cannot merge with amber.
2. **Hue families are chosen, not optimised.** amber / teal / neutral / violet / crimson.
   An optimiser left free put `working` and `limited` in the same violet family — it scored
   well and looked like a mistake.
3. **Thresholds by consequence.** Tier A (15 dE2000, every view): every pair involving
   `awaiting_user`, plus `terminated` against `working` and `idle`. Tier B (8): the rest,
   where both members mean "nothing is demanded of you". In greyscale only
   `awaiting_user`/`terminated` is tier A, because it is the only pair sharing both a solid
   fill and a blink and therefore the only one with nothing but colour left.

Both themes pass with margin. The exact hexes and the measured worst pair per view are in
[ui-overlay.md](../ui-overlay.md) §3.

**`working` moves off lime onto teal.** That is the one semantic change, and it is forced by
the 6.2 above.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Keep the provisional palette | No work | It fails three of its own document's stated requirements, measurably |
| Pick hues that look right, adjust by eye | Fast, and how most palettes are made | Both attempts failed on a number nobody would have seen by eye — 0.0 dE in greyscale between two colours that look obviously different in colour |
| One blanket threshold for every pair | Simple to state | Five statuses, one lightness axis and — under red-green CVD — effectively one usable hue axis. A blanket 15 is unsatisfiable, and a blanket 8 lets the summons blur into a crash. Consequence is the thing that actually differs between pairs |
| Require all five to be separable **by luminance alone** | The strongest possible greyscale guarantee | Not physically available in the usable range once 3:1 contrast is also required. Claiming it would be the same false precision this project refuses elsewhere. Silhouette carries it instead, and §3.1 makes no two statuses share one |
| Invert the dark palette for the light theme | One palette to maintain | On paper every indicator must be *dark* to clear 3:1, so the lightness ladder cannot also carry urgency. The light theme keeps the hues and the thresholds but not the ladder |

## Consequences

**Better**

- FR-16 and FR-17 become **checkable without a screenshot**. The thresholds are computable
  from the palette constants alone, so they become a unit test in phase 4 rather than a
  reviewer's opinion ([test-plan.md](../test-plan.md) §7).
- A regression is loud: changing one hex fails a named pair in a named view.
- D-2 closes with the reasoning recorded, so the next person changing a colour knows what
  the colour was carrying.

**Costs and risks accepted**

- **The margins are thin** — the worst pair clears its threshold by about 1 dE2000. There is
  very little room to make a colour prettier without failing something, which is a real
  constraint on future taste.
- The simulation matrices model *dichromacy at full severity*. Anomalous trichromacy is more
  common and less severe; this is the conservative case, not the typical one.
- CIEDE2000 was designed for large flat samples under controlled lighting, not for 10 px
  chips on an arbitrary monitor. It is a proxy. It is a much better proxy than an opinion,
  and it is the only one available without user testing.
- `unknown` on the light theme ends up *darker* than `idle`, which is the wrong direction for
  a recessive status. There is no lighter grey that clears 3:1 on paper. Its thin dotted ring
  keeps the ink mass low instead.

**Triggers to revisit**

- User testing, or a user reporting a confusion the numbers said was safe.
- A tier assignment turning out to be wrong — that is a change to this ADR's premise, so it
  needs a new ADR, not an edit here.
- The board gaining a size at which the glyph becomes legible, which would add a fourth
  channel and loosen everything.
