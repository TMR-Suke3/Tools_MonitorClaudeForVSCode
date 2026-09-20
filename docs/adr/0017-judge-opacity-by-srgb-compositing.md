# ADR-0017: Judge opacity against sRGB compositing, not linear light

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Refines**: [ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md), whose decision
  stands unchanged. One measured figure it quotes is corrected, and the model that produced
  it is named so the number can be checked

## Context

[ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md) retired aging by brightness.
One of its two supporting measurements was that `idle` at `#676B6F` **"drops under the 3:1
floor at any opacity below 87 %"** — a figure repeated in
[ui-overlay.md](../ui-overlay.md) §3.1 and in [test-plan.md](../test-plan.md) §7.1 as
TC-52b.

Writing TC-52b as an actual test made the figure checkable for the first time, and it does
not reproduce. **Where a translucent indicator crosses the floor depends on how the
compositing is done, and the two models disagree by six percentage points:**

| Model | `idle` crosses 3:1 at | Contrast at 87 % |
|---|---|---|
| Linear light — physically correct, and what the design spike used | **86.6 %** | 3.01:1 |
| 8-bit sRGB channels — what CSS `opacity` does | **92.2 %** | 2.80:1 |

87 % is the linear-light answer, rounded. It is a correct number about the wrong renderer:
the board is a WebView ([ADR-0004](0004-use-tauri-and-rust.md)), so an `opacity` on an
indicator is composited by the browser in the 8-bit sRGB channel values, gamma and all.
Under the model that will actually run, `idle` is already under the floor at 90 %.

Nothing about ADR-0013's decision depends on which figure is right — both put the retired
55 % ramp far under the floor (1.87:1 sRGB, 2.27:1 linear), and both say there is no gentler
setting that works. But an uncorrected number in a document that says *"measured"* is a trap:
the next person to check it either reproduces the wrong model or concludes the document is
unreliable.

## Decision

**Contrast claims about a translucent indicator are evaluated with sRGB compositing, and the
published crossing point for `idle` becomes 92 %.**

- `docs/ui-overlay.md` §3.1 and `docs/test-plan.md` TC-52b are corrected to 92 %.
- [ADR-0013](0013-make-idle-the-faintest-thing-on-the-board.md) is **not** edited. Its 87 %
  is what was measured when it was written, and the repository's rule is that an ADR records
  a decision as it was made. This ADR is where the correction lives.
- The core carries **both** compositing functions, `composite_srgb` and `composite_linear`,
  with the sRGB one documented as the model that matters. TC-52b asserts only what holds
  under both, so the test does not quietly depend on this choice being right.

No requirement changes. FR-16 delegates the palette's numbers to `ui-overlay.md` §3.2 and
states none of its own, so correcting a measurement there needs no edit to
[requirements.md](../requirements.md).

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Keep 87 % and note that models differ | No number changes | The board has one renderer. "It depends" is not a floor a test can defend, and the figure would stay wrong for the model that ships |
| Standardise on linear light and require the board to composite that way | Physically correct; blending artefacts are genuinely better | It means opting every translucent element out of the browser's default compositing, for a property no requirement asks for, to defend a number rather than a behaviour. The tail wagging the dog |
| Drop the crossing point from the documents entirely and assert only "no opacity ramp exists" | Nothing left to be wrong | The figure is *why* the ramp cannot come back. Deleting it leaves the decision looking like taste, which is what ADR-0013 was written to avoid |
| Edit ADR-0013 in place | One number, one file | The repository's ADR rule forbids it, and rightly: the correction is more legible as "this was measured under a model that turned out not to be the renderer's" than as a silent overwrite |

## Consequences

**Better**

- The published figure now describes the renderer the product uses, and a test reproduces it.
- The compositing model is named. It was an unstated assumption in both directions before —
  the spike assumed linear, the board would have used sRGB, and nothing said so.
- The gap between the models is itself useful: 6 points of opacity is enough to flip a
  pass/fail on a 3:1 floor, which is worth knowing before any other translucent element is
  specified.

**Costs and risks accepted**

- **ADR-0013 now contains a figure that this ADR corrects**, and a reader who stops there
  gets the old one. The link from ADR-0013 forward does not exist, because ADRs are not
  edited; the entry point is this file and the corrected `ui-overlay.md`.
- The two compositing functions in the core are near-duplicates, and only one is used by the
  product. That is the price of a test that does not depend on the choice.

**Triggers to revisit**

- Any translucent element being specified — a fade-in, a hover state, a click-through mode
  (FR-31). Each one inherits this model, and the 3:1 floor applies to it under the same
  arithmetic.
- The board ceasing to be a WebView, which would change the compositing model wholesale
  ([ADR-0004](0004-use-tauri-and-rust.md) names the conditions under which that is revisited).
