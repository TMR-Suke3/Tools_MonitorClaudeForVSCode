# ADR-0009: Map a session to its window through the extension host, and verify the raise

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author

## Context

FR-33 — clicking a session brings the VS Code window that owns it to the foreground —
became a v1 `MUST`. That promoted open decision **D-3** to blocking: nobody had established
that the right window could be reached at all, and the design pass could not fix a click
target until it was.

Three things were unknown, and a fourth was assumed wrongly.

- **One VS Code process hosts several windows.** `observation-sources.md` §2.2 described
  `~/.claude/ide/<port>.lock` as mapping "a VS Code process to the workspace folders that
  window has open", which quietly assumes the process *is* the window.
- **`SetForegroundWindow` is not always granted.** Windows refuses it for background
  processes, and FR-30 requires the board never to take focus — so the board is, by
  construction, in the refused category.
- **Two windows on the same folder** (limitation L-1) were known to be indistinguishable
  from the session data.
- The requirement carried a **prepared retreat**: if no mapping existed, weaken FR-33 to
  "raise one of the windows that has this workspace open".

Measurements are in the working notes (§18); the scripts are `25_windows.py` …
`32_ptyhost.py`.

## Decision

**Identify the window through the process tree and the lock's port; actuate through the
window title; and verify the result by reading the foreground window back.**

```
sessions/<pid>.json  ->  claude.exe
   parent            ->  the extension host  (one per WINDOW)
   listening port    ->  ide/<port>.lock     -> that window's workspaceFolders
   title match       ->  HWND
   AttachThreadInput + SetForegroundWindow
   read GetForegroundWindow back after 250 ms -> tell the user whether it worked
```

FR-33 stands **unchanged**; the prepared retreat is not taken.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Use the lock's `pid` as the window key | Already in the file | It names the *instance*. Of 38 pids across 82 lock files, **31 carried more than one lock** — up to five, always on different workspaces. It cannot distinguish windows at all |
| Match the session's `cwd` against `workspaceFolders` | Simple, no process table | Ambiguous exactly when it matters (two windows, one folder), and it compares paths, which the encoding gotchas make fragile. The process route is both exact *and* cheaper |
| Ask the extension over the lock's WebSocket port | It is the window's own IPC, so it could focus precisely | Needs the `authToken`, which rule 2 of `observation-sources.md` forbids reading. Also an undocumented internal protocol |
| `code --reuse-window <folder>` | No Win32 at all | Same folder-level granularity, spawns a process per click, and **opens a new window** when nothing matches — strictly worse than matching a title |
| Plain `SetForegroundWindow` | The obvious call | Measured **0 of 3** with an unrelated app in the foreground. `SPI_GETFOREGROUNDLOCKTIMEOUT` on the test machine is 2147483647 ms: the lock never expires |
| Synthetic `ALT` keypress, or a `TOPMOST` z-order dance | Both measured 3 of 3 | The keypress injects a real input event that can open menus; the z-order dance mutates the target window's state. `AttachThreadInput` reaches the same result with neither side effect |
| Spawn a helper process per click to make the call | Measured 3 of 3 — a new process gets a one-shot foreground privilege | A process launch on every click, for something a documented API call already does |

## Consequences

**Better**

- Session → window is **exact**, and stays exact when two windows have the same folder open.
  That is stronger than FR-33 promises, and it costs one parent-pid lookup plus the
  listening-port table — both already needed for the process probe of
  [ADR-0008](0008-detect-blocking-by-whether-the-tool-started.md).
- D-3 closes without weakening a requirement.
- **A failed raise is detectable.** `ui-overlay.md` §9 assumed it was not, and had listed
  "a raise that silently fails looks identical to one that worked" as an open problem. The
  return value really is worthless — `FlashWindowEx` returned `TRUE` on all three runs while
  raising nothing — but comparing `GetForegroundWindow()` to the target after a short delay
  is reliable. The board can therefore acknowledge success *and* report failure, which fixed
  the click-feedback design rather than merely constraining it.

**Costs and risks accepted**

- **Actuation is only folder-accurate.** The extension host owns no window; the window
  belongs to a renderer, a sibling process, and nothing at the OS level joins the two. The
  window title is the only route, and it carries the folder's basename. So L-1 survives, and
  a new limitation L-6 appears: two windows whose folders share a last path segment.
- **The title is a user setting.** Anyone who customises `window.title` breaks the match.
  Mitigated by the verification step: the raise then fails *visibly* instead of silently.
- **The parent-process link is measured only for extension-launched sessions.** A `claude`
  started by hand in an integrated terminal descends from the terminal's host process
  instead, and whether that host is per-window was not tested. The `cwd` match remains as a
  fallback for exactly that case.
- Every part of this reads Claude Code and VS Code internals, so [C-3](../requirements.md)
  applies in full.

**Triggers to revisit**

- The lock file gains a genuine per-window identifier, or the extension exposes a documented
  focus command.
- VS Code stops putting the workspace name in the default window title.
- A measurement of terminal-launched sessions shows the parent chain does not identify the
  window, which would make the `cwd` fallback the primary route rather than the backstop.
