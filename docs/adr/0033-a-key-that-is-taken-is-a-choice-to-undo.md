# ADR-0033 — A key that is taken is a choice to undo, not a fault to report

- **Status**: Accepted
- **Date**: 2026-09-06
- **Requirement**: FR-61, FR-28
- **Refines**: [ADR-0031](0031-one-corner-for-every-summons.md), whose summons is what one of
  these keys asks for

## Context

The board registers two global shortcuts. They were chosen "to be free rather than to be
memorable" — `Ctrl+Alt+…`, the range Windows itself leaves alone — and that reasoning is sound
about *Windows*, which is not who else is on the machine.

A user reported the second key opening a different application. The keys had been given to
something else long before this product was installed, and three things followed, each worse
than the last:

1. **There was no way to change them.** They are settings, they persist, and nothing in the
   product edits them. The only remedy was to hand-edit a JSON file the product never
   mentions the existence of.
2. **The menu blamed the wrong thing.** Registration answered with one boolean for the pair,
   so a key held by another application made the menu report *both* as not registered —
   including the one that worked perfectly.
3. **A key that could not be parsed disabled the other one.** The registration returned on the
   first parse failure, before the second key had been offered at all. A typo in one setting
   silently turned off a shortcut that had nothing wrong with it.

There is also a question underneath all three: *is this key free?* It cannot be answered by
looking at the key. The only authority is the operating system, and the only way to ask is to
ask for the key.

## Decision

**The keys are changeable and clearable, from a screen in the setup window** — reached from
*Change these keys…*, which sits directly beneath the two lines it repairs.

**Every outcome is measured, and reported per key.** Five states: *working*, *another
application has it*, *the board's other shortcut has it*, *not a key this can use*, and *not
set*. The screen shows what came back from the registration it just performed, not what was
intended.

The third of those is the board's own collision, and it is separate from the second because
the repair and the blame are both different: to the operating system a key the other row
already holds refuses exactly as a key another application holds does, and reporting it that
way would send someone hunting through their own machine for a culprit that is us.

**A cleared key is a supported answer.** Blank means nothing is registered and nothing is
wrong.

**A shortcut needs at least one modifier.** A bare letter registered globally is that letter
taken away from every other program on the machine. The rule is enforced where the key is
*offered* rather than only where it is chosen: `M`, `F5`, `` ` `` and `Escape` all parse as
good accelerators, and a settings file can be written by hand, copied from another machine, or
left behind by an older build — none of which passes through the window that asks for the
key.

## Consequences

**"Not set" had to be a first-class state, not an error path.** A user whose keyboard is
already spoken for is better served by a board with no shortcut than by one that keeps
fighting for a key — and a product that reported their own decision as a failure would be
arguing with its owner. That is why the answer is a state per key rather than a boolean and a
message.

**Five states, because the fifth has a different culprit.** *Live*, *off*, *taken* and
*unreadable* were the four this decision started with. Both rows set to the same key is the
fifth: the second registration fails exactly as a taken key does, but the application holding
it is this one. Reporting it as *taken* would send someone hunting through their own machine
for a culprit that is us, so it says so and names the row that has it.

**The defaults do not change.** `Ctrl+Alt+M` and `Ctrl+Alt+B` stay, because changing them
would move the keys under everyone who already has them working, to fix a collision that is
specific to one machine. The collision is now something its owner can resolve in two clicks,
which is the right place for a machine-specific answer to be made.

**The registration is per key, and the parse is separated from the asking.** *Blank* and *not
a key* can be decided by looking at the string, which is what makes TC-130 … TC-136 testable
without a keyboard or a machine that happens to have the right key spare. What cannot be
decided that way is left to the operating system, deliberately: the product does not guess at
whether a key is free, and there is no list of "keys other applications usually take" anywhere
in it — such a list would be wrong on the first machine that disagreed with it.

**The screen states what it cannot know.** "The board cannot tell whether a key is free until
it asks for it" is written on it. This is the same honesty FR-52 requires of hook mode: say
what is being inferred and what is being observed, and never present the one as the other.
