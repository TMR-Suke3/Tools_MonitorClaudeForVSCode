# ADR-0015: The folded board drops titles and narrows to its workspace names

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Refines**: [ADR-0012](0012-one-density-that-folds.md), which decided that folding
  replaces the minimal density. It folded the board's **height** only

## Context

[ADR-0012](0012-one-density-that-folds.md) replaced the two densities with one that folds.
It shrank the board from 15 rows to 4 — but only vertically. The width stayed at a constant
300 px, and its Consequences recorded the cost honestly: fully folded, 300 × 90 against the
old indicator strip's 232 × 22, *about five times the area*.

Reviewing it, the author asked for the folded board to be narrower, and pointed at the one
thing still holding it wide: `edge-node`'s row read

```
● edge-node   port the sensor driver              05:50
```

`port the sensor driver` is a **session title**. It is there because a workspace with one
session is drawn as a single merged row (ADR-0012), and that row shows both the workspace
name and the title. On a fully folded board it is the only title left — every other row is a
group header — and a single long title holds the whole board wide while nothing else needs
the width.

## Decision

**A fully folded board draws no session titles, and its width is set by its content: the
longest workspace name, clamped to [140, 300].**

Three parts:

1. **The merged row drops its title while the board is folded.** It keeps the workspace name
   and the time. It has no fold state of its own — that is still ADR-0012 — but it follows
   the *board's* state. The full title stays one hover away (FR-22), and the time stays
   because for `limited` the reset clock is the actionable half.
2. **Two discrete widths, not a continuous fit.** Any group open → 300. Everything folded →
   fit the names. **The folded width is recomputed only when the set of workspaces changes**,
   never on a status change and never on a title change.
3. **The board resizes away from the edge it is snapped to.** Snapped right, it grows left;
   snapped to the bottom, it grows up. A board parked in a corner must not walk off the
   screen when a group is unfolded.

Measured on the twelve-session example, whose longest name is `monitor-claude` (84 px),
after the horizontal spacing was tightened in the same pass — the earlier figures were
175 × 90 and 17 %:

| State | Size | Area |
|---|---|---|
| Every group folded | **160 × 90** | **15 %** |
| One group open | 300 × 150 | 48 % |
| Every group open | 300 × 310 | 100 % |

This also corrects ADR-0012's own cost figure: the folded board is about **three** times the
area of the strip it replaced, not five.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Keep 300 px always | One number, no resize behaviour at all | It is the width of a title column that a folded board does not draw. Paying for a column that is not there is the whole complaint |
| Fit the width continuously to whatever is on screen | Always exactly as wide as needed | Titles change mid-session (FR-23 says so explicitly, and requires the row not to reflow). A board that re-measured on every title change would twitch in the corner of the eye all day. Two widths cost one rule and no twitch |
| Truncate the merged row's title to some small budget instead of hiding it | Keeps a hint of what the session is | A title cut to five characters is not a hint, it is noise — and FR-19/FR-20 already say truncation is only ever *to fit*, never an editorial act. Hidden and one hover away is the honest version |
| Hide the merged row's title always, in every state | One rule, no board-state dependency | Then a one-session workspace never shows what it is doing without a hover, which is a real loss on an open board that has the width to spare |
| Abbreviate long workspace names to cap the width | Bounded width whatever the folder is called | FR-04 requires the folder's own name and FR-19 forbids shortening. The same reasoning that keeps session titles intact applies to folder names |

## Consequences

**Better**

- The folded board is **15 % of the fully open one** by area, and the gap to the strip
  ADR-0012 replaced closes from ~5× to ~3×.
- The width now says something: it is the length of the user's own folder names. Nothing
  arbitrary is being reserved.
- The resize rule makes corner placement safe, which the constant width had let the spec
  avoid answering.

**Costs and risks accepted**

- **The board is only as narrow as the longest folder name.** One workspace called
  `Tools_MonitorClaudeForVSCode` sets the width for every other row, and by FR-04 and FR-19 the
  product will not abbreviate it. A user with long repository names gets little of this.
- **The board changes size on fold and unfold**, which is motion the old constant width did
  not have. It is user-initiated, so it never happens under the pointer unexpectedly — but a
  height cap firing automatically (§2.5) can now change the width too.
- A merged row looks different depending on the board's state, which is a second thing a
  reader has to learn about a row kind that already had a special case.

**Triggers to revisit**

- Long folder names in practice making the folded width useless, which would reopen
  abbreviating them — and that is a change to FR-04, not to this decision.
- A measured complaint about the resize itself, which would argue back towards one constant
  width at the folded size rather than at 300.
