# ADR-0026 — The diagnosis lives in the setup window

- **Status**: Accepted
- **Date**: 2026-08-28
- **Requirement**: FR-55; [hook-setup.md](../hook-setup.md) §5

## Context

FR-55 asks for two things, and only the first was built: the mode must be visible from the
menu at all times, *together with a diagnosis view that reports each precondition and the
single action that repairs it*. The menu says **what** the board is doing — "waiting for you
is exact", or "worked out from a pause". It does not say **why**, and the why has four
different answers with four different repairs:

| Precondition | What repairs it |
|---|---|
| `mcv-hook.exe` is beside the board | Reinstalling the app. Nothing the product can do. |
| `~/.claude/settings.json` can be read | The user opens it (FR-53's own action). |
| Our entries are in it | Run the setup. |
| Events are arriving | Nothing decidable from here — see below. |

Two questions had to be settled: where the panel lives, and what the fourth row does.

## Decision

**The panel is a step in the existing setup window**, reached from a new menu item —
*Check the reporting…* — and openable directly at `setup.html?step=diagnose`.

The alternative was a window of its own. Against it: this window already has the theme, the
chrome, the step machinery, and — the part that decides it — **every repair the panel offers
is already a path through this same window**. "Set it up…" is the preview step. "Open the
file" is the refusal step's own button. A separate window would have had to either duplicate
those or hand off to this one on every click. Hook mode gets one place where it is explained
and repaired, rather than two that have to agree.

**The fourth row watches rather than diagnoses.** Entries installed and nothing arriving has
two causes that are indistinguishable from the outside: no session has started since the
entries went in (the commonest, and `hook-setup.md` §7 says so), or something is stopping the
helper. Naming either would be a guess. So the row's single action is *Watch for the next
one* — it waits for a real event, exactly as step 5 does after an install (FR-50), and says
which it was. **Proof, not inference**, in the one corner of the product where an inference
would be presented as a fact.

Three rules the rows follow, each of which was a way to get this wrong:

- **A row that holds offers nothing.** A repair button beside a tick is how a panel stops
  being read.
- **A repair that would be refused is not offered.** With the helper missing, the entries row
  states what is true and offers no button: setting up would write five entries naming a
  program that is not there, and `hook_preview` refuses it anyway. The repair belongs to the
  row above.
- **Two rows never offer the same repair.** With nothing installed, the silence row is not
  the place to act — the entries row is. "The single action" stops being single the moment it
  appears twice.

**A settings file that does not exist is not a failure.** A first install writes the file
Claude Code would have written itself. Marking that row broken would send the user to open a
file that is not there, while the row below already offers the thing that creates it.

## Consequences

- FR-55 is met in full, and the deferral table loses its last hook-mode row.
- **The panel is a pure function.** `setup::diagnose` takes `Facts` and returns the rows;
  `hook_diagnosis` gathers the facts. The split is what makes it testable: none of the four
  situations it exists to describe could be produced on a developer's machine without
  breaking that machine's real settings file.
- One flaw this surfaced and fixed: **the event log outlives the entries that filled it**.
  Remove the entries and a report from a minute ago is still in the file, so a row that read
  only the newest timestamp said the reporting was fine on the same screen as "none of them
  are there". Nothing installed is now judged before the log is read at all.
- The window now opens on one of two screens, which the screenshot scripts need to reach
  (`--diagnose`, `.notes/tools/shoot-setup.ps1 -Step diagnose`). A screen that cannot be
  photographed is a screen that stops being reviewed.
- The panel is read-only until a button is pressed, and every button leads to a path that
  already asks for consent. Nothing here installs, removes, or writes on its own (FR-54).
