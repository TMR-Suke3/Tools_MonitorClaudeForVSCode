# ADR-0014: The prominence ladder pins the two ends, not the middle

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Refines**: [ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md) and
  [ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md). Neither decision is
  reversed; the principle both of them cited is corrected, because the palette does not obey
  it and — deliberately — will not

## Context

[ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md) states the rule the palette
is built on as **"prominence follows urgency"**, and
[ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md) used the same rule to push
`idle` to the contrast floor.

Reviewing the design, the author asked a question the documents had no honest answer for:
*what decides a folded group's colour, and what happens to a session that has finished?*
Answering it exposed that the palette does not follow the rule it claims.

A folded group takes the status of its most-demanding session, by the roll-up precedence in
[session-state-model.md](../session-state-model.md) §8:

```
awaiting_user  >  terminated  >  limited  >  idle  >  unknown  >  working
```

`working` is **last** — it asks for nothing. But measuring what the board actually draws
(ink index = indicator area × contrast against its own background, dark theme):

| | Order |
|---|---|
| Urgency (roll-up) | awaiting_user > terminated > limited > idle > unknown > **working** |
| Prominence (ink) | awaiting_user 1771 > **working 824** > limited 446 > terminated 361 > idle 212 > unknown 106 |

`working` is last in urgency and **second in prominence**. The visible consequence, in the
committed images: a group of three `working` sessions folds to a bright teal row, while a
group of three `working` **plus one `idle`** folds to the faintest grey on the board. The
group with strictly more in it looks calmer.

Colour alone cannot fix it. Dimming `working` as far as every threshold allows reaches
`#09A3AC`, 5.80:1, ink 580 — still second. The cause is **area**: `working` is a solid
10 × 10 chip at 100 px², against `limited`'s 91 and `idle`'s hollow 64. Only a smaller mark
would close the gap, and that was offered.

**The author's answer was that `working` may be prominent.** So the rule is what is wrong,
not the palette.

## Decision

**The ladder pins the two ends by urgency and leaves the middle to legibility. Say that,
instead of claiming a total ordering the palette does not have.**

1. **`awaiting_user` is the loudest thing the board can draw** — brightest, largest (it is
   the only status with a halo), and it blinks. Nothing may approach it.
2. **`idle` is the faintest** — the resting state does not compete
   ([ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md)).
3. **Between those two, prominence is not an urgency ranking.** Each status is as prominent
   as it needs to be to be read:
   - `terminated` is dark because its hue must stay clear of amber under colour-vision
     deficiency; **its prominence comes from the blink**, not from luminance.
   - `working` is bright because *how much is alive right now* is information the board
     exists to carry, not a summons. It pulses; it never blinks.
   - `unknown` carries the least ink of all six despite a mid luminance, because it is a
     fallback and must not read as an alarm.
4. **The roll-up precedence is an information rule, not a loudness rule.** It decides *which*
   status stands for a folded group. It does not promise that one folded group looks louder
   than another, and the reading it supports is the right one: a bright teal row means *this
   workspace is busy*, a faint grey row means *something here finished and nothing is being
   asked of you*.

Underneath all of it is the sentence the board is actually built on, which no document had
stated plainly: **the alarm channel is amber and crimson plus motion. Everything else is
ambient.** The user scans for two colours; the rest is context.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Dim `working` to `#09A3AC` (5.80:1) | Closes part of the gap | Still second in prominence, so the claim would still be false. A change that buys inconsistency at a lower brightness is worse than either end |
| Shrink `working` to a 6 px dot (ink 297) | Would make the claim true — order becomes awaiting > limited > terminated > working > idle > unknown | Rejected by the author: a running session should read plainly. It also costs the thing the board is best at — showing at a glance how much is alive |
| Re-rank the roll-up so `working` outranks `idle` | Makes the two orders agree | Semantically wrong. `idle` means *maybe look at me*; `working` means *leave me alone*. A folded group should surface the first |
| Say nothing and leave the claim in place | No work | The claim is load-bearing: it is the stated reason `terminated` is dark and `idle` is at the floor. A future reader following it would "fix" `working` and break the board |

## Consequences

**Better**

- The documents describe the board that exists. The contradiction was reachable by anyone
  who compared §8 of the state model with §3.2 of the overlay spec.
- The two constraints that actually matter — `awaiting_user` at the top, `idle` at the
  bottom — are now stated as the invariant, which is also what a test can assert.
- ADR-0013's decision survives intact, and reads better: `idle` is at the floor because the
  *resting state* should not compete, not because of a total ordering that was never true.

**Costs and risks accepted**

- **A folded group of busy sessions looks louder than a folded group that also has a finished
  one.** That is the concrete oddity this ADR declines to fix. It is defensible under the
  reading in Decision 4, and it is unmeasured — nobody has watched a user misread it.
- "Prominence is legibility in the middle" is a weaker rule than "prominence follows
  urgency", and weaker rules constrain future changes less. The two pinned ends and the tier
  thresholds of ADR-0010 are what remain enforceable.

**Triggers to revisit**

- A user reporting that a busy workspace pulled them away from one that needed them — the
  failure this ADR accepts the risk of.
- Any future status added between the ends, which would have to justify its own prominence
  rather than inheriting a place in an ordering.
