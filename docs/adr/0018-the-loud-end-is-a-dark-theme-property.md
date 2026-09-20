# ADR-0018: The loud end of the ladder is a dark-theme property; on paper the halo carries the summons

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Refines**: [ADR-0014](0014-the-ladder-pins-the-ends-not-the-middle.md), whose decision
  stands. Its first pinned end is stated without a theme, and holds on only one of them

## Context

[ADR-0014](0014-the-ladder-pins-the-ends-not-the-middle.md) corrected an overclaim: the
palette does not make prominence follow urgency throughout, so the ladder pins its two ends
and leaves the middle to legibility. The two ends it pins are:

1. **`awaiting_user` is the loudest thing the board can draw.** Nothing may approach it.
2. **`idle` is the faintest.**

Both are stated without naming a theme, and ADR-0014's evidence — the ink index table,
indicator area × contrast against the board — is the **dark** board only.
[ui-overlay.md](../ui-overlay.md) §3.2 repeats the claim the same way.

Turning it into TC-52c made the light theme measurable, and end 1 does not hold there:

| Dark board | Light board |
|---|---|
| **awaiting_user 1771** > working 824 > limited 447 > terminated 361 > idle 212 > unknown 107 | **terminated 727** > limited 582 > **awaiting_user 535** > working 510 > idle 254 > unknown 135 |

`awaiting_user` is **third of six** on paper. This is not a slip in the palette; it is
forced. On a light board every indicator has to be *dark* to clear 3:1 against the page, so
contrast against the background rises as a colour gets darker — and `awaiting_user` is amber,
the one hue that cannot go dark without becoming brown and colliding with `terminated` under
colour-vision deficiency. The rung that makes it loudest on a dark board is exactly the rung
that makes it quiet on a light one. Every threshold in §3.3 still passes; the light palette
is not the problem.

The same recomputation confirms end 2 on **both** themes: `idle` is the faintest of the five
displayed statuses on paper as well (254, against 510 for the next), which is what
[ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md) set out to do.

## Decision

**End 1 is scoped to the dark theme. On the light theme the summons is carried by area,
silhouette and motion instead of by ink, and one theme-independent invariant is stated in
its place.**

1. **Dark board — `awaiting_user` has the highest ink index of all six.** Unchanged; this is
   ADR-0014's end 1, now with a theme on it.
2. **Both boards — `awaiting_user` is the largest mark the board draws**, at 160 px² against
   `working`'s 100 and `idle`'s 64. The halo is what makes it so, it is theme-independent,
   and it is the property that survives the light theme's inverted contrast. This is the
   quantified form of "the only status with a halo", which §3.1 already required.
3. **Both boards — `idle` is the faintest of the five displayed statuses.** Unchanged, and
   now confirmed on paper as well.
4. **On the light board, no claim is made about where `awaiting_user` sits in the ink
   ordering.** It sits third, that is a consequence of the contrast inversion, and inventing
   a threshold to dress it up would repeat the mistake ADR-0014 was written to correct.

The alarm channel is unchanged and is what actually carries the summons on either board:
**amber and crimson plus motion.** On a dark board amber is also the brightest thing present;
on a light board it is not, and the halo and the blink do the work. That is a difference in
*which* channel dominates, not in whether the user can find the row.

No requirement changes. FR-16 delegates the palette to `ui-overlay.md` §3.2 and states no
numbers; FR-17 already requires the halo as a distinct silhouette, which decision 2 quantifies
rather than alters.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Re-derive the light palette so `awaiting_user` is loudest there too | The claim becomes true on both themes with no scoping | It requires amber to go dark enough to out-contrast `#9B001C`, which lands it in brown — and brown against crimson is the tier-A confusion the whole palette was rebuilt to avoid ([ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md)). Trading a real separation for a tidy sentence |
| Enlarge the light-theme halo until ink ordering agrees | Area is the free variable; no hue moves | The indicator would differ in size between themes, so the board's geometry would depend on the colour scheme. §2 fixes the geometry once, deliberately |
| Drop end 1 entirely and pin only `idle` | Simplest; nothing left to scope | End 1 is load-bearing on the dark theme, which is the default and the one every committed image shows. Deleting it licenses a future change that dims the summons |
| Leave the claim unscoped and let the test cover only the dark theme | No documents to change | This is the state the tests found, and it is the one ADR-0014 explicitly warns against: "the claim is load-bearing … a future reader following it would 'fix' `working` and break the board". The same argument applies to a reader who takes end 1 to the light palette |

## Consequences

**Better**

- Three invariants that a test can assert, each with the themes it applies to spelled out:
  ink on the dark board, area on both, `idle` on both. TC-52a and TC-52c cover them.
- The light theme gains a guard it did not have. Before, nothing at all constrained
  `awaiting_user`'s prominence on paper; now its area is pinned as the largest on the board.
- The reason the light theme inverts is written down. It follows from the 3:1 floor and the
  amber hue, and it will otherwise be rediscovered by whoever next edits that palette.

**Costs and risks accepted**

- **On a light board the summons is not the loudest mark**, and that is now documented rather
  than fixed. A user on the light theme finds `awaiting_user` by its halo, its blink and its
  hue — not by it being the most contrasting thing present. Unmeasured: nobody has watched a
  user scan a light board.
- The invariant set is now three items with different scopes, which is harder to keep in the
  head than "the two ends". The tests are where it is enforced, not memory.
- `terminated` being the loudest thing on a light board is left standing. It is defensible —
  it blinks, and it means a session died — but it was not chosen, it fell out.

**Triggers to revisit**

- The light theme becoming the default, or the images in `docs/images/` being redrawn on it.
  The costs above are acceptable for a secondary theme and would want re-examining for a
  primary one.
- Any change to the halo, which is now the load-bearing channel on one of the two themes.
