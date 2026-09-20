# ADR-0032 — One board per machine, and a second launch is a summons

- **Status**: Accepted
- **Date**: 2026-09-06
- **Requirement**: FR-60, FR-32
- **Rests on**: [ADR-0031](0031-one-corner-for-every-summons.md), whose summons this reuses

## Context

Two boards were found running on the author's machine while a different defect was being
investigated. Both had been there for some time. Nothing on screen said there were two.

What two boards do to each other is worth writing down, because none of it announces itself:

- **They restore to the same remembered position.** Measured: both at 1221, 702, one drawn
  exactly on top of the other. The board is frameless and has no taskbar button, so the second
  is not something a user can see, cycle to, or close.
- **They write the same settings file.** Position, fold state and size are last-writer-wins
  between two processes that never look at the file again after startup.
- **Only the first gets the shortcuts.** `RegisterHotKey` is first-come-first-served, so the
  second board's registration fails and its menu says *not registered* — while the keys work
  perfectly well, for the other board. A user reading that menu is being told something true
  about a process they do not know exists, and false about their keyboard.
- **The tray gets two icons**, drawn identically, one of which controls a window nobody can
  see.

The failure this produces is not "two windows". It is a set of symptoms that all point
somewhere other than their cause.

## Decision

**A second launch does not start a second board.** `tauri-plugin-single-instance`, registered
before every other plugin, holds the guard; a later process hands its arguments to the running
board and exits.

**What the second launch asks for is the summons** — the board is shown and brought to the
top-right of the display the pointer is on, exactly as the tray click and the shortcut do
(ADR-0031).

## Consequences

**A dev build and an installed build can no longer run side by side.** They share an
identifier, so the guard treats them as the same product — which they are. This is the one
real cost, it is paid by whoever is working on the board rather than by whoever uses it, and
the answer is to close the installed one first.

**Starting the product twice is now a feature rather than a mistake.** Double-clicking the
icon of a resident tool that is already resident is a request to see it, and it is the same
request the tray icon and the shortcut make. It is answered the same way, which also means
there is now a route back to a lost board that needs no tray icon, no menu and no remembered
key: run it again.

**The plugin, not a mutex of our own.** A named mutex is ten lines and would have covered the
first half — stopping the second process — but not the second half, which is the useful one:
telling the first that somebody asked for it. That needs the argument-passing channel the
plugin already implements and tests on three platforms.

**The guarantee is "one board", not "one board at every instant".** On Windows the plugin
takes a named mutex first and creates the hidden window that carries the summons second, so a
launch arriving in the gap between those two sees the mutex but has nowhere to send its
request — and carries on starting. The window is opened within milliseconds of the mutex, the
gap exists only while the first board is itself starting, and a second launch inside it is a
double-click fast enough to beat a process launch. It is named here because it is the one hole
in the rule this ADR states, and because the fix, if it ever matters, is not ours to make: it
is in the plugin, and working around it locally would mean the mutex of our own that the
paragraph above argues against. TC-128 does not cover it — the case waits for the first
board's window before launching the second, which is precisely the state the race cannot
happen in.

**The test launches the real binary twice** (TC-128, TC-129). Whether a second *process* stops
itself is a fact about two processes and cannot be asked of a function, so this is one of the
few cases in the product where the automated test runs the product. It skips, with a printed
reason, where there is no desktop to open a window on — and where a board is already running
and holds the guard, which is the state a developer's own machine is usually in.
