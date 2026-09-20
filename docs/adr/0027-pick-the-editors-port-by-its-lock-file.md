# ADR-0027 — Pick the editor's port by its lock file, not by being first

- **Status**: Accepted
- **Date**: 2026-08-28
- **Requirement**: FR-33
- **Refines**: [ADR-0009](0009-map-a-session-to-its-window.md), which is otherwise unchanged

## Context

ADR-0009's chain maps a session to a window through a listening port:

```text
sessions/<pid>.json  ->  claude.exe
   parent            ->  the extension host — one per WINDOW
   listening port    ->  ide/<port>.lock  ->  that window's workspaceFolders
   title match       ->  HWND
```

The step that reads the port was written on a stated assumption, and the code said so in as
many words:

> Only IPv4 listeners, which is what the extension host uses. A process with several is not a
> case that occurs here — the host listens on one — and taking the first is honest about that
> rather than pretending to choose.

**That is false, and it was measured false on the author's own machine.** One extension host
(pid 30520) was listening on **three** ports — 54851, 58915 and 59126 — and only 59126 had a
lock file. A second host was listening on two, 16147 and 63698, with the lock under 16147.
The order the TCP table returns them in is not meaningful, and the first is not the one
Claude Code registered.

The consequence was the worst shape a bug can have here: **clicking a session did nothing at
all.** The chain took the first port, found no lock claiming it, and returned `NotFound` — so
every click on every session of that window was a silent no-op, which is indistinguishable
from a board that is ignoring the mouse.

It also hid behind the diagnostic meant to find it. `--check-raise` printed one port and the
line *"no lock claims that port"*, which is a true statement about a port nobody should have
been looking at, and which reads as "Claude Code has not registered this window".

## Decision

**Ask every port the host is listening on, and take the first one a lock file claims that
also names a folder.**

- `win::listening_ports_of` returns all of them. It does not choose; choosing is not
  something the process table has the information to do, because the ports carry no label.
- `raise::title_needle_of` walks them and asks the lock files. **The lock files are the
  authority**, and they are the right authority: a lock is written by the very thing being
  looked for.
- A port with no lock, and a port whose lock names no folder, are both passed over rather
  than treated as an answer.
- If more than one port of one host ever has a lock, the first wins. That is stated rather
  than guarded: both would belong to the same editor instance, so the wrong answer would be a
  window of the right application — the failure mode ADR-0009 already accepts as L-1, not a
  new one.

`--check-raise` now prints **every** port with what its lock says, and distinguishes *no lock
file* from *a lock that names no folder*. The second is a real and ordinary state — an editor
window with no folder open — and it is a different problem from the first.

## Consequences

- Clicking a session raises its window again. Verified on the machine that produced the
  measurement: the chain now resolves 59126 and finds
  `"Tools_MonitorClaudeForVSCode - Visual Studio Code - Insiders"`.
- **One real case still ends in `NotFound`, correctly.** An editor window with *no folder
  open* has an empty `workspaceFolders`, so there is no folder basename in its title to match
  on. Nothing can be raised for it, and the board says so visibly rather than raising
  something else (TC-75). That is limitation L-6, not a defect against this decision.
- The assumption that failed was **written down, and that is what made this findable**. The
  lesson is not to stop writing assumptions down; it is that a documented assumption about
  someone else's process is a measurement with an expiry date. `docs/observation-sources.md`
  §2.2 now carries the count.
- Nothing about ADR-0009's chain changes. This refines one step of it; the ADR stands.
