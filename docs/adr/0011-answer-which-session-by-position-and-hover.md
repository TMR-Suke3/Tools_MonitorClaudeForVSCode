# ADR-0011: Answer "which session?" by position and hover, not by making the indicator say more

- **Status**: **Superseded** by [ADR-0012](0012-one-density-that-folds.md) — the minimal density this decision was about no longer exists, so the problem it solves does not arise. Kept because the alternatives it weighed are the same ones ADR-0012 had to weigh again
- **Date**: 2026-08-26
- **Deciders**: the author

## Context

`ui-overlay.md` called this "the design question that decides whether the product is
usable", and it is the only one it described that way.

Minimal density draws **every** session on the machine at one 10 px indicator each — that is
FR-18's definition of the density and the scope rule the whole product runs on. With four
workspaces open that is a plausible twelve indicators in a 232 × 22 px strip. The board's
job is to answer *which session needs me?*, and half of that answer — *which* — has no
obvious carrier at that size:

- Colour is fully spent on status (FR-16) and cannot also encode identity.
- Order must be stable, because urgency is expressed by colour and motion and never by
  position (FR-23). So the board cannot sort the interesting one to the front.
- There is no text at this density.

Four directions were on the table: grouping rhythm, fixed slots the user learns, size or
brightness by recency, and a hover that expands the group in place.

## Decision

**Minimal density answers *does anything need me?* exactly, and *which one?* by position.
The exact answer costs one hover. The indicator is not made to carry identity.**

Three mechanisms, all of which had to be present together:

1. **Chunking.** A 10 px gap and a hairline between workspace groups. Twelve
   undifferentiated marks cannot be read at a glance; four chunks of two to four can.
2. **Slots that hold still.** Group order is **first-seen and persisted** — not alphabetical,
   not by urgency. A group keeps its slot while it has any session, and a workspace that
   disappears and returns within 10 minutes is given its old slot back. Within a group,
   sessions are ordered by start time ascending and never move.
3. **Hover promotes one group in place**, after 120 ms, as an overlay that leaves the board's
   footprint and the other groups exactly where they were.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Size or brightness by recency | Genuinely helps find the fresh one | Brightness is already spent on `T_dim` aging, and size changes move an indicator's neighbours — which destroys mechanism 2, the thing that makes position mean anything |
| A one-character workspace initial at minimal density | Answers *which* directly, and cheaply | It stops being "indicators only" — it changes FR-18's definition of the density, and it makes the strip grow per group. The density exists to be ignorable; text is what the compact density is for |
| A per-workspace accent colour on the separator | Does not touch the indicator | A second colour system competing with the status palette, in a design where five statuses already only just clear their separation thresholds ([ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md)) |
| Sort the urgent session to the front | Zero-effort identification | Contradicts FR-23 outright. Muscle memory is worth more than one saved glance, and a board that rearranges itself is one the user has to re-read every time |
| A tooltip only, with no in-place expansion | Already specified, no new mechanism | A tooltip answers about one indicator. The question is usually *which of these*, which needs the group shown together |

## Consequences

**Better**

- Position becomes reliable enough to learn, because nothing reorders it: not status, not
  urgency, not a session ending in another group.
- Hover-to-expand gives an exact answer with no click and no permanent size cost, so the
  minimal density can stay genuinely minimal instead of growing towards compact.
- The product does not claim a precision it lacks, which is the same discipline
  [ADR-0005](0005-no-goal-achievement-status.md) applies to statuses.

**Costs and risks accepted**

- **A 10 px indicator on its own still does not say which project it belongs to.** That is
  the decision, not a defect, and `ui-overlay.md` §10 says so to the user's face.
- Persisted slot order is state the board must keep and migrate, and a stale slot map is a
  new way for the layout to be surprising. The 10-minute reclaim window is a guess and is not
  measured.
- Hover-to-expand needs the board to accept mouse-over, which interacts with the eventual
  click-through mode (FR-31): in click-through, mechanisms 1 and 2 are all the user gets.
- The three mechanisms are unvalidated by any user other than the author. This is a
  reasoned design, not a tested one.

**Triggers to revisit**

- A user with more than about six workspaces open at once — chunking degrades when the
  number of chunks itself stops being countable.
- Telemetry-free evidence that hover is not being discovered, which would argue for the
  workspace initial after all, and therefore for changing FR-18.
