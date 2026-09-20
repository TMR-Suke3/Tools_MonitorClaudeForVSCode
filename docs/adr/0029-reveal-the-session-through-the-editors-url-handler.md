# ADR-0029 — Reveal the session through the editor's URL handler

- **Status**: Accepted
- **Date**: 2026-08-28
- **Requirement**: FR-33, FR-28
- **Refines**: [ADR-0009](0009-map-a-session-to-its-window.md), whose rejection of the lock's
  WebSocket stands

## Context

FR-33 states a limit as part of the requirement rather than as a defect against it: *raising
a window cannot select which session inside it is shown*. That was true of every route
[ADR-0009](0009-map-a-session-to-its-window.md) examined. The one that could have done it —
the lock file's WebSocket — was rejected because it needs the `authToken`, which rule 2 of
`observation-sources.md` forbids reading, and that rejection has not changed.

Measuring the installed extension found a different route, and a later search of the
documentation found that it is **a supported feature with a published contract**
([Use Claude Code in VS Code](https://code.claude.com/docs/en/vs-code), *Launch a VS Code tab
from other tools*). `anthropic.claude-code` 2.1.246 calls `window.registerUriHandler` in
`extension.js` — `package.json` does not declare it, which is what made it look private — and
it accepts:

```
vscode://Anthropic.claude-code/open?session=<id>&prompt=<text>
  -> commands.executeCommand("claude-vscode.primaryEditor.open", session, prompt)
  -> createPanel(session, …)
       sessionPanels.get(session)  ->  panel.reveal()
       otherwise                   ->  a new panel for that session
```

Verified on a live machine, in this order, because each step could have ended it:

| Question | Answer |
|---|---|
| Does an external URL reach the handler? | Yes, after the editor asks the user's permission. Its prompt has a *don't ask again* box |
| Does `session=` select a session, or just reveal the active view? | **It selects.** Sending one session's id and then another's acted on each in turn |
| Does the message box get focus? | **Yes**, as part of the reveal. The second half of what was wanted arrives free |
| Can the *window* be aimed at? | **No**, and the documentation says so: *"the URL opens in whichever window is currently focused"* |
| What happens when it goes to the wrong window? | **It opens that session there.** Measured, and documented: *"If the session isn't found, a fresh conversation starts instead"* |

The last row is the whole difficulty. This is not a feature that fails by doing nothing; it
fails by rearranging an editor — which is exactly what FR-33's 2026-08-27 clarification
forbids, and what TC-101 exists to hold.

The documentation also states the precondition that decides *when it can fail that way*:

> The session must belong to the workspace currently open in VS Code. If the session isn't
> found, a fresh conversation starts instead. If the session is already open in a tab, that
> tab is focused.

So the duplicate is not bad luck. It is what the handler is specified to do when a session is
sent to a window whose workspace it does not belong to, and it can be **predicted** from two
things the board already reads: the session's `cwd`, and the window's `workspaceFolders`.

## Decision

**Build it, off by default, and send the URL only in the moment the board can prove it is
aimed.**

Five conditions, each load-bearing, and any one failing means the click does what it always
did — the window comes forward and nothing else happens:

1. **The user turned it on.** It is off until they do.
2. **The raise was verified.** `win::raise` already reads the foreground window back rather
   than trusting a return value (ADR-0009); only `Outcome::Raised` proceeds.
3. **That window is still the foreground one at the instant of sending.** Asked again rather
   than carried over from the raise. This is the only lever there is over which window
   answers.
4. **The session belongs to that window's workspace**, compared between the session's `cwd`
   and the lock's `workspaceFolders`. This is the documented precondition, and checking it is
   what turns the duplicate-tab hazard into a case the board simply declines. A window with
   **no folder open** never passes, which is correct: the editor could not resume into it
   either.
5. **The editor's URL scheme is one this build knows**, taken from the lock's `ideName`. An
   editor it does not know is not guessed at.

**The URL goes to the editor's own launcher, not to the shell's URL association.** This was
found the hard way: the board logged every send and nothing happened, because
`HKCR\vscode-insiders` runs `Code - Insiders.exe --open-url` and that executable answers
`bad option: --open-url`. Only `bin/*.cmd` works — it sets `ELECTRON_RUN_AS_NODE` and hands
the arguments to the editor's `cli.js`. Going through the launcher **beside the running
editor's own executable** also aims better than the association could: the URL reaches the
installation whose window was just raised, rather than whichever copy owns the scheme. Where
`bin/` holds more than one candidate — a real install also had `new_code-insiders.cmd`, a
staged update — the ambiguous ones are skipped and anything still ambiguous is declined.

**The consent is the editor's, not ours.** The board's toggle is a preference; the permission
that matters is the editor's own prompt, which the user grants and can withdraw in the editor.
This product writes nothing to the editor's settings to arrange it — a better position than
the hook setup, which has to edit a file and therefore has to back it up (FR-37).

**The warning is given before the button, not after the surprise.** The consent screen states
that the session may open in a different window of the same editor, because this product does
not present a confidence it is not earning (FR-52's argument, applied here).

`prompt=` is **not used**. The board has no business typing for the user.

## Consequences

- Clicking a session takes you to the session, not merely to the window it is in — which is
  what the board was for.
- **A new dependency on a published contract**, which is a better footing than this ADR first
  claimed. It is still the only part of the product that drives another application rather
  than observing it, and everything around it is built to fail quietly: an unknown editor
  name, a lost foreground, a session outside the workspace, an id that is not id-shaped, a
  launcher that cannot be identified — all end in nothing happening, and each says which on
  stderr.
- **The delay is the editor's, and cannot be removed.** The launcher takes about 1.2 s to
  return on the machine measured, because it boots Electron as Node to run `cli.js`; the URL
  is dispatched somewhere inside that. The board's own contribution was a fixed 250 ms wait
  before it could confirm the raise, and that is gone — it now confirms by looking every
  10 ms. Nothing else on this path belongs to the board.
- **The order cannot be reversed.** The window must be focused before the URL is sent,
  because focus is what aims it, so the window always comes forward before its session's tab
  does.
- **FR-28 gains an entry**, and it is a boolean — less than any other thing the board writes
  down.
- The failure mode that remains is the race in condition 3, which is far narrower than the
  one this ADR was first written against: with the workspace check in place, a mis-aimed URL
  can only reach a window whose workspace the session also belongs to. If it proves common,
  the honest next step is still to withdraw the feature rather than add heuristics on top of
  a guess.
- The board still never opens a window the user did not ask for: every path here begins with
  a click on a session that is already on the board.
