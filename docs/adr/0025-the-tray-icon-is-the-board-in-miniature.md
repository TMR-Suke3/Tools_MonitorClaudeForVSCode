# ADR-0025 — The tray icon is the board in miniature

- **Status**: Accepted
- **Date**: 2026-08-28
- **Requirement**: FR-32

## Context

FR-32 asks for the board to be *reachable from a tray icon when hidden*. Until now that was
comfort — the board could be closed and launched again. It stopped being comfort when
`Ctrl+Alt+B` shipped on this branch: a global shortcut that hides the board leaves the only
way back inside the user's memory, and a resident tool whose window can be dismissed needs
somewhere it still lives.

Reaching the board is the easy half. The question that needed deciding is whether the icon
should **carry a status**, and if so, how it can carry one honestly.

The case for carrying one is the product's own thesis. A hidden board answers nothing, and
the measurement behind FR-34 says the gap is real: across 78 transcripts the user left a
*finished* session sitting for more than two minutes on 271 of 503 turn ends. An ambient
board only helps while it is being looked at.

The case against is a measurement problem. Every separation the palette rests on is stated
against **the board's own background** ([ADR-0010](0010-derive-the-palette-from-a-luminance-ladder.md),
`ui-overlay.md` §3.3): contrast at 3:1, tier-A pairs at 15 dE2000 and tier-B at 8, under
normal vision and three simulated deficiencies. All of it is measured on one surface. A
coloured disc on a transparent tray icon does not sit on that surface. It sits on the
taskbar, whose colour the user chooses, which may be tinted by an accent, and against which
nobody has measured anything. Shipping it would be presenting a confidence the product is
not earning — the failure FR-52 exists to forbid, in a corner FR-52 does not cover.

## Decision

**The icon carries the roll-up status, drawn as a small picture of the board**: the panel's
own fill, the panel's own edge around it, and the mark in the middle.

- The pair of colours on screen is then the pair §3.3 validated — indicator against
  `theme.board()` — so there is no new measurement to make and none to skip.
- The edge does a second job: it separates the tile from a taskbar that may be near enough
  to the board's own background to swallow it.
- The roll-up is `mcv_core::machine::roll_up` over every session on the board, computed in
  `view::build` beside the groups' own roll-ups. The tray does not decide which of two
  statuses is louder; `session-state-model.md` §8 decides, in one place.
- **`unknown` is drawn as a plain ring, where the board draws a dotted one** (§3.1). This is
  the one deliberate departure. At the size the icon is actually shown the dots are
  sub-pixel, and a dotted ring that resolves to a smudge reads as a filled disc — which would
  claim a definite status exactly where the meaning is that there is not one. A ring says
  "nothing solid here" at any size.

**Start-on-login, FR-32's second half, is not part of this** and stays deferred. It means
writing to the user's `Run` key or Startup folder, and this product does not touch the user's
configuration without the explain-consent-undo flow that
[ADR-0003](0003-guided-hook-setup.md) specifies and FR-54 requires. That flow is a screen's
worth of work of its own. FR-32 is a SHOULD, the deferral table keeps the row, and nothing
here makes it harder to add.

## Consequences

- A hidden board still answers "is anyone waiting for me?", which is the question the whole
  product is for.
- The icon inherits the palette's guarantees rather than needing its own. If a status colour
  changes, the icon changes with it and TC-48 … TC-51 still cover the pair on screen.
- **What is not inherited is motion.** `awaiting_user` expands a wave on the board and
  `terminated` blinks (§3.1); the tray icon does neither. It is a bitmap the shell owns, and
  animating it would mean handing the shell a new one several times a second. The colour is
  the only channel here, which is the same reduction §3.1 already requires to lose no
  information under reduced motion — so the substitute is one the palette is already measured
  for.
- The icon is composited by hand into an RGBA buffer, so it is the one surface this product
  draws that cannot be seen by running it. `mcv-board --tray-icons` prints every icon as
  pixels for that reason, on the same argument that keeps `--palette`: the cheapest way to
  check a picture against what the code holds.
- The tray's menu cannot be rebuilt at the moment it opens, the way the right-click menu is
  (§6.2) — the shell opens it without asking. It is replaced instead whenever one of the
  statements on it stops being true: the mode, the size, and whether the shortcuts
  registered.
