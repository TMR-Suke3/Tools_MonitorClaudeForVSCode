# ADR-0001: Observe Claude Code through its local state files, with hooks as an opt-in upgrade

- **Status**: Accepted
- **Date**: 2026-08-24
- **Deciders**: Repository owner (@TMR-Suke3)

## Context

The product has to know, in near real time, what every Claude Code for VS Code session on
the machine is doing — including sessions in VS Code windows that are not focused, and
several sessions inside one window.

Constraints found while surveying what is actually available:

- **There is no public API for this.** Claude Code exposes no query interface, and the
  VS Code extension offers no third-party surface for reading session state.
- **Claude Code does keep local state**, in three places that together cover what is
  needed: a per-process session registry, an editor/window registry, and an append-only
  transcript per session that is written as the conversation happens. Details in
  [observation-sources.md](../observation-sources.md).
- **Claude Code also supports user-configured hooks** that fire on lifecycle events
  (session start/end, prompt submitted, tool calls, notifications, stop). These are exact
  and immediate — but they require editing the user's own settings file.
- The observer must never interfere with the sessions it watches, and must not send the
  user's prompts or source code anywhere (NFR-04, NFR-07, NFR-08).
- None of the state files are documented or version-stable.

The forcing question: does the product *require* hooks to be installed?

## Decision

**Read Claude Code's local state files as the baseline, and treat hooks as an optional,
explicitly opted-in accuracy layer.**

- With zero configuration, the product works: it discovers sessions, groups them, and
  derives every status by inference from the state files.
- If the user opts in, the product installs hooks that report lifecycle events to a local
  file. Where a hook signal exists it overrides the inferred one.
- Hook installation backs up the file it edits, is reversible from the UI, and leaves any
  hooks the user already configured untouched (FR-37).
- Reading is strictly read-only, incremental (tail by byte offset), and never touches
  credentials or internal IPC endpoints.

## Alternatives considered

| Option | Upside | Why it was not chosen alone |
|---|---|---|
| **Local state files only** | Zero configuration; no writes to the user's environment at all. | Cannot distinguish "a permission prompt is on screen" from "a slow tool is running" — precisely the transition the product exists to surface. |
| **Hooks required** | Exact, immediate, low-latency signals for every transition. | Installation is a write into the user's Claude Code settings before the product has earned any trust; it can collide with existing hooks; a broken hook degrades the tool being watched. Too high a price for first contact. |
| **A companion VS Code extension** | Officially supported extension APIs; runs inside the window it observes. | Would need to be installed in every window, cannot draw a free-floating always-on-top window across windows, and still has no access to another window's Claude session state. |
| **Connect to Claude Code's internal IPC** | Richest possible signal. | Private, undocumented, credentialed channel. Connecting to it makes the product a client of an interface it has no right to, and one bad message could disturb a live session. Ruled out on principle, not on difficulty. |
| **Screen scraping / accessibility APIs** | Sees literally what the user sees, including UI state. | Fragile, language- and theme-dependent, expensive, and reads the whole screen. Worst option on both robustness and privacy. |

## Consequences

**What gets better**

- The product runs the moment it is launched, on any machine, with nothing to configure —
  which is what makes it worth using at all.
- No writes to the user's environment happen without an explicit decision.
- Users who want exact "waiting for you" detection have a supported path to it, and the
  cost of that path is visible to them.
- Observation stays entirely local, which lets the whole product hold the "no network"
  guarantee (NFR-08).

**What it costs**

- Two code paths for status derivation (inferred and event-driven), both of which need
  tests.
- In the default mode, `awaiting_user` is a debounced inference with a false-positive
  window; the state model has to expose that uncertainty in the UI rather than hide it.
- The product depends on undocumented internals and **will** break on some future Claude
  Code release. That is accepted and designed for: version is recorded, parsing is
  defensive, and unknown data degrades to a neutral status instead of a wrong one
  (NFR-05, FR-39).

**When to revisit**

- Claude Code publishes a documented API, a status/IPC contract, or an extension surface
  for session state → prefer it over file inference immediately.
- The file formats change in a way that makes inference unreliable enough that hooks become
  effectively mandatory → reconsider making hook installation part of first-run setup.
