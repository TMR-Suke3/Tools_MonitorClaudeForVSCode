# ADR-0028 — Name the window with no folder by elimination

- **Status**: Accepted
- **Date**: 2026-08-28
- **Requirement**: FR-33
- **Refines**: [ADR-0009](0009-map-a-session-to-its-window.md) and
  [ADR-0027](0027-pick-the-editors-port-by-its-lock-file.md), neither of which changes

## Context

ADR-0009's chain ends by matching the workspace folder's basename against a window title.
That step has nothing to work with when the editor window **has no folder open**:

- `~/.claude/ide/<port>.lock` carries `workspaceFolders: []` — measured, 3 of 84 accumulated
  locks;
- and the window's title carries no folder either. One measured live read was
  `"メモを整理して - Visual Studio Code - Insiders"` — the file on screen, and nothing else.

So there is no string to match on, and the chain correctly returned `NotFound`. Correctly,
but uselessly: the board drew two sessions in that window, and clicking either did nothing.
From the user's side a row that cannot be clicked is a broken row, whatever the reason.

Three things were measured before choosing an answer, and each ruled something out:

- **The instance does not narrow it.** Both extension hosts (51212 and 30520) were children
  of the same `Code - Insiders.exe`, pid 32788. "Descends from the editor" is true of every
  window of both.
- **The renderer does not link back to its host.** The hosts run as `--type=utility` with no
  window id; the renderers carry `--vscode-window-config=<guid>`, and two different windows
  shared one guid. Nothing joins a host to its own renderer.
- **The windows are not even owned by the renderers.** Both visible windows reported pid
  32788 — the main process — so window ownership does not distinguish them either.

`messagingSocketPath` in the lock file might have. It is not read, by rule 2 of
`observation-sources.md`, and that rule is not worth spending on a convenience.

## Decision

**Name the window by what it cannot be.**

An extension host is per window, so *n* live hosts on one instance means *n* windows. Take
the instance's visible windows, strike out every one whose title carries the folder of some
**other** host of the same instance, and if **exactly one** remains, it is this session's.

Not by recognising it — nothing here recognises it — but because nothing else can be it.

**Exactly one, or nothing.** Two windows left means two sessions could own either, and this
product does not raise a window it cannot name: a wrong window is worse than a visible
failure (TC-75). Zero left is a stale process table, and must not fall through to "raise the
first one". Both end as `NotFound`, which the board shows.

That is the rule limitation L-1 already states for two windows sharing a folder, applied to
the case where the folder is **absent** instead of duplicated.

## Consequences

- The sessions in a folderless window can be clicked. Verified on the machine that produced
  the measurement: both `C:\Users\someone` sessions resolve to the one unclaimed window, and
  the window with a folder still resolves by name as before.
- **A new limitation, L-7**: two folderless windows on one editor instance cannot be told
  apart, and neither is raised. It is stated rather than guarded against, because it is the
  same shape as L-1 and has the same answer — the board carries enough information that the
  jump is a shortcut, not the way it is read (FR-33).
- The elimination costs one extra pass over the instance's other hosts, and only on the path
  where the ordinary match already failed. Nothing changes for a window with a folder.
- `--check-raise` walks the same arithmetic and prints it: how many windows the editor has,
  which are spoken for, and whether one is left. The failure this replaced was invisible from
  the outside, and the next one in this area should not be.
- **It inherits the looseness of the title match, and in a new place.** Striking a window out
  uses `title_matches`, which is deliberately a containment test — `window.title` is a user
  setting, so ADR-0009 answers imprecision with the verification step rather than a cleverer
  match. Here that means a needle of `board` also strikes out a folderless window showing
  `board-notes.md`. Almost always that is harmless: one window too many is struck out, none
  is left, and nothing is raised. It could name the wrong window only if the *folder* window's
  title had stopped carrying its own folder — which is a customised title, the case where the
  ordinary path fails visibly too. Raised in review 2026-08-28; recorded rather than guarded,
  because tightening the match is the thing ADR-0009 declined to do.
- **This is inference, and the board's own vocabulary applies to it.** It is not on the same
  footing as the port-and-lock chain above it, which is exact. What keeps it honest is that
  the raise is verified afterwards either way: the board reads the foreground window back and
  says whether it came (ADR-0009), so a wrong guess here would be visible rather than
  silently believed.
