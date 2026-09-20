# ADR-0021: The `working` indicator does not animate

- **Status**: Accepted
- **Date**: 2026-08-27
- **Deciders**: maintainer

## Context

`working` pulsed: 1.6 seconds, 100 % down to 55 % opacity and back, for as long as a session
was doing anything. The reasoning was written down in
[ui-overlay.md](../ui-overlay.md) §3.1 and in
[ADR-0014](0014-the-ladder-pins-the-ends-not-the-middle.md) — *"it reassures, it does not
summon"* — and it was a reasonable thing to believe before anyone had watched the board for a
day.

Watching it for a day is what changed the argument, and the numbers say why. `working` is not
an exceptional state: across the recorded corpus there were roughly 503 turn ends against 16
detectable waits (`docs/test-plan.md` §6.1, `session-state-model.md` §4.1), so the statuses
the board spends its life showing are `working` and `idle`. A pulse on `working` is therefore
not an occasional signal — it is **the board's resting appearance**, a permanently moving
thing sitting always-on-top in the corner of the user's eye.

That is the failure the specification already names for a different rule: *"a board where
everything moves teaches the user to ignore it"* (`session-state-model.md` §1). Motion was
rationed against `idle` and `unknown`, but the most common status kept it.

[ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md) anticipated this. Its rejected
alternatives include *"make `working` recessive too"*, turned down as a different argument and
marked **"worth revisiting if the board reads as busy"**. It reads as busy.

## Decision

`working` is drawn **static, at full strength**, in the same teal it already has. The
rendering that was the reduced-motion substitute becomes the only rendering, and `Motion::Pulse`
leaves the vocabulary entirely rather than staying as an option nothing chooses.

## Alternatives considered

| Option | Good | Why not |
|---|---|---|
| Keep the pulse | No change; the reassurance argument is real | It is the resting appearance of the board, not an event. Perpetual peripheral motion is what teaches a user to stop looking |
| Slow it down (4–6 s) | Calmer, keeps *some* life | Still moving, and a slow pulse is harder to read as deliberate than either a still mark or a clear blink. It trades a loud mistake for a vague one |
| Pulse only for a few seconds after a transition | Motion where there is news | The board already answers "what changed" with the time column, which is text and does not move. Adding motion back for the same job doubles the channel and re-introduces movement on the commonest transition of all |
| Dim `working` as well as stilling it | Even calmer | Rejected for ADR-0013's reason, unchanged: *how much is alive right now* is information this board exists to carry. Brightness is how it carries it |

## Consequences

**Better**

- **Motion now means exactly one thing: the developer has to move.** `awaiting_user` and
  `terminated` are the only statuses that animate for attention, and `limited`'s drain is a
  countdown rather than a summons. The rationing rule in §1 is now true of the whole board
  rather than of most of it.
- The board is still in the ordinary case. A row moves when something has happened, and at no
  other time.
- One fewer animation running forever in a webview that sits on top of everything.

**Worse, or newly owed**

- `working` loses a channel. FR-17 asks for status to survive without colour, and for
  `working` that now rests on the silhouette (a filled chip, unique) plus lightness. This is
  not new ground: §3.1's reduced-motion column already read *"none needed — hue and lightness
  already separate it"*, so the accessibility claim was never leaning on the pulse. What
  changes is that every user now gets what the reduced-motion user got.
- **ADR-0014 says "It pulses; it never blinks."** That sentence is now wrong, and it is not
  rewritten: ADR-0014's *decision* — that the ladder pins its ends and not its middle — is
  untouched and stands. Only that supporting detail is superseded, here.
- A session that is working and one that has just been discovered look identical. They always
  did; the pulse never distinguished them.
