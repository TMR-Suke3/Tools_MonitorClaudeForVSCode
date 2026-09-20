# ADR-0012: One density that folds, instead of two densities

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Supersedes**: [ADR-0011](0011-answer-which-session-by-position-and-hover.md)

## Context

FR-18 required two densities: **minimal**, indicators only, and **compact**, indicator plus
title plus time. Minimal existed for one reason — to be small enough to leave in a corner
permanently.

It was expensive for that.

- It needed a **second complete layout**: its own geometry, its own click target
  (16 × 22 px slices), its own overflow rule, its own place to put the usage-limit
  countdown, its own answer for the empty state.
- It could not say **which workspace an indicator belonged to**. Twelve identical 10 px
  squares in a 232 px strip, with colour already spent on status and order forbidden from
  expressing urgency (FR-23). [ADR-0011](0011-answer-which-session-by-position-and-hover.md)
  spent three mechanisms — chunking, persisted slots, hover-to-expand — recovering an
  approximate answer, and still had to end with "a 10 px indicator does not say which
  project it belongs to".
- `ui-overlay.md` called that "the design question that decides whether the product is
  usable". It was the hardest thing in the design, and it existed only because the density
  existed.

Reviewing the first design pass, the author asked for the compact density alone, plus the
ability to fold. The two requests turn out to be one idea: **folding does what minimal was
for, and does it without giving up the words.**

A second observation came with it. A workspace with **one** session was being drawn as a
group header saying "1" above a single row — two rows to say what one row says, for what
the lock-file corpus shows is the commonest shape of group.

## Decision

**One density. Groups fold to a single row and unfold again. A workspace with one session is
drawn as a single merged row with no fold state at all.**

Measured from the specification, twelve sessions across four workspaces:

| State | Rows | Size |
|---|---|---|
| Every group folded | **4** | 300 × **90** |
| One group open, the rest folded | 7 | 300 × 150 |
| Every group open | 15 | 300 × 310 |

Two supporting rules make it work:

- **Headers fold; rows raise.** A click on a group header folds or unfolds and never raises
  a window; a click on a session row or a merged row raises the window (FR-33). Without that
  split, one gesture would have to mean two things.
- **A folded group still carries status**, through the roll-up indicator and the state
  model's precedence. Folding hides *which* session needs the user, never *whether* one does.
  That is what makes folding safe to leave on.

FR-18 is rewritten accordingly. [ADR-0011](0011-answer-which-session-by-position-and-hover.md)
is superseded rather than deleted: its alternatives table is the record of what was weighed,
and half of it had to be weighed again here.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Keep both densities, add folding to compact | Nothing is lost | Two layouts to build, test and keep consistent, for a density whose only advantage — size — folding now provides. Every UI rule would need saying twice |
| Keep minimal, drop compact | The smallest possible board | It is the half that cannot name a workspace. Dropping it is the point |
| Fold a one-session group like any other, header and all | One rule, no special case | The header would say "1" above the row that says everything. It doubles the height of the commonest group to display a number the row already implies |
| Show the merged row as folder name only, unfolding to reveal the title | Literally "auto-folded, click to open" | Then a one-session workspace hides its title behind a click for no saving — the row is the same height either way. Merging shows both and costs nothing |
| Auto-fold groups with nothing urgent in them | Smallest useful board, no interaction | The board would resize itself whenever a status changed, so the row under the pointer moves as you reach for it. The height cap (§2.5) gets most of the benefit and only fires when the board would otherwise be too tall |
| One row per **session**, no groups at all | Simplest of all | Loses FR-04 grouping and FR-07's "two editors on one folder are one group", and 12 sessions would always be 12 rows |

## Consequences

**Better**

- **The hardest design problem is gone**, not solved: nothing has to identify a workspace
  from a 10 px square, because every entry on the board carries its workspace name or sits
  under a header that does.
- **The click target grows from 352 px² to 6000 px²** — a whole row. `ui-overlay.md` §6.1
  previously had to argue that the hit area must exceed the drawn indicator "because a miss
  that drags the window is the worst outcome". That argument is now moot.
- One layout to specify, build and test. Three test cases disappear and are replaced by
  fold/unfold cases that are easier to state.
- Persisted slot order, the 10-minute reclaim window and hover-to-expand all leave with
  ADR-0011 — three mechanisms, one of them an unmeasured guess, no longer needed.

**Costs and risks accepted**

- **The board is bigger.** Fully folded it is 300 × 90 px against the old strip's 232 × 22 —
  about five times the area. Anyone who wanted a genuinely tiny always-on-top sliver does not
  get one. This is the trade being made deliberately: legible beats small.
- **A folded group hides which of its sessions is urgent.** The roll-up says one is; finding
  out which costs a click.
- **Fold state is persisted state**, so it can be restored wrong, and it needs migrating if
  the shape changes.
- The one-session merged row is a **second row grammar**, which is a special case however
  well justified — a reader has to learn that a row without a chevron is not foldable.

**Triggers to revisit**

- A user who genuinely needs a smaller footprint than 300 × 90, which would reopen the
  indicator-only density and with it every problem ADR-0011 catalogued.
- Workspaces routinely holding one session each, which would make the group header rare
  enough to question whether grouping is earning its rules.
