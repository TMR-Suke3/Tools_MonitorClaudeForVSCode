# ADR-0013: Make `idle` the faintest thing on the board, and retire aging by brightness

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Refines**: [ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md) — the method is
  unchanged; one rung of its ladder moves, and one rule that depended on the old rung goes

## Context

[ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md) states the principle the
palette is built on: **prominence follows urgency.** The palette it produced did not obey it.
`idle` — the least urgent of the five statuses, the one that means *nothing is being asked of
you* — was placed **third brightest**, at `#92969B`, L\* 62, 5.97:1.

`ui-overlay.md` defended that placement in as many words: "**`idle` is not dim.** It is the
third-brightest, because `idle` is the 'maybe needed' answer, not 'nothing to see':
measurement found the user left a finished session sitting for more than two minutes on 271
of 503 turn ends."

The measurement is real. The inference from it was not. **271-of-503 argues for the opt-in
alert (FR-34), which is exactly what it was collected for** — it says the user is often not
looking at the board at all, and a brighter resting colour does nothing for someone who is
not looking. What it costs is paid every second the user *is* looking: `idle` and `working`
are the two states most sessions are in most of the time, so making one of them the third
loudest thing on screen is most of what the board looks like.

Reviewing the design, the author asked for a finished session to be as unobtrusive as
possible. That is not a preference overriding a measurement; it is the palette's own stated
principle, applied where the first pass failed to apply it.

A second thing fell out of the change. `ui-overlay.md` had an aging rule: after `T_dim`
(30 min) an idle indicator dropped to 55 % opacity. That is incompatible with a recessive
`idle` — measured, at `#676B6F` the indicator is at 3.31:1, and **any opacity below 87 %
drops it under the 3:1 floor** the same document requires.

## Decision

**`idle` becomes the faintest of the five, at the contrast floor. Aging by brightness is
retired.**

| | Was | Now |
|---|---|---|
| `idle`, dark | `#92969B`, L\* 62, 5.97:1 | **`#676B6F`, L\* 45, 3.31:1** |
| `idle`, light | `#73787C`, 4.09:1 | **`#767A7E`, 3.96:1** |
| Ladder, dark | awaiting > working > **idle** > limited > terminated | awaiting > working > limited > terminated > **idle** |
| Aging | `T_dim` 30 min → 55 % opacity | none |

Two supporting points, both measured:

- **Prominence is contrast × area.** Ink index on a 10 px indicator, dark theme:
  `awaiting_user` 1771, `working` 824, `limited` 447, `terminated` 361, **`idle` 212**,
  `unknown` 106. `idle`'s hollow silhouette and its colour compound; neither alone would do
  it.
- **Age did not need brightness any more.** `T_dim` existed to say "this is old news" in a
  density that had no room for text. That density is gone
  ([ADR-0012](0012-one-density-that-folds.md)), and **every row now shows its age in the time
  column**, exactly, as a number. The opacity ramp was a leftover.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Leave `idle` bright, rely on `T_dim` to recede it | No palette change | It only recedes after 30 minutes, so the first half hour — when most sessions are — is the loud case. And it contradicts ADR-0010's stated principle for the whole of that time |
| Push `idle` below 3:1 | Genuinely invisible until looked for | Breaks the floor `ui-overlay.md` §8 sets for every indicator. `idle` is a *state*, and it is the only thing distinguishing "finished" from "gone"; a user who cannot see it cannot tell those apart |
| Keep `T_dim` at a gentler 80 % | Preserves the aging idea | Measured: the floor is crossed at 87 %. There is no gentle setting that works |
| Make `working` recessive too | Even calmer board | Not asked for, and it is a different argument: `working` says *this is alive*, and pulses to say it. Worth revisiting if the board reads as busy, but not folded into this decision silently |
| Show age by desaturating instead of dimming | Keeps contrast | `idle` is already neutral — its chroma is capped at 3. There is nothing to desaturate |

## Consequences

**Better**

- The palette now does what ADR-0010 says it does, in the one place it did not.
- **Separation from `awaiting_user` in greyscale doubles**: 15.1 → **31.3** dE2000. Pushing
  the resting state down pushes the summons further away from it, so the change that makes
  the board quieter also makes the one alarming colour easier to find.
- One timing constant and one exemption rule disappear. `limited` no longer needs to be
  explicitly exempt from an aging rule that would have taken a three-hour countdown dark
  half way through.
- Every threshold of ADR-0010 still passes, both themes, with the same ~1 dE margin.

**Costs and risks accepted**

- **A finished session is now easy to overlook**, which is the point, and is a real loss for
  anyone who was using the board to notice completions. FR-34's opt-in alert is the intended
  answer, and it is off by default — so a user who wants to be told has to turn it on.
- **`idle` sits at the contrast floor**, 3.31:1 against a 3:1 requirement. There is no margin
  left: any future change that darkens the board background or the indicator fails the floor.
- **Age is no longer visible peripherally.** Reading it means reading the time column.
- On the light theme `idle` barely moved — 4.09:1 to 3.96:1. The lightness range on paper is
  narrow and `awaiting_user` is close above it, so **there the silhouette does nearly all the
  work.** Stated rather than papered over.

**Triggers to revisit**

- Sessions finishing without being noticed *while the user is watching the board* — that
  would mean the alert is not covering the case this decision hands to it.
- Any change to the board background, which would move the contrast floor `idle` is sitting
  on.
