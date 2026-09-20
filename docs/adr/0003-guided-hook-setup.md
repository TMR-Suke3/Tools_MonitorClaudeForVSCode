# ADR-0003: Ship the hook setup as a guided, reversible flow instead of documenting it

- **Status**: Accepted
- **Date**: 2026-08-24
- **Deciders**: Repository owner (@TMR-Suke3)

> **Amended by [ADR-0008](0008-detect-blocking-by-whether-the-tool-started.md) (2026-08-25).**
> The Context below argues for hooks partly because inference is "a debounced guess up to
> 45 s late, shown with reduced confidence". That rule has since been replaced: inference
> now waits for positive evidence instead of elapsed time. The decision to ship guided hook
> setup still stands — hooks remain authoritative and immediate — but the *strength* of the
> argument below is reduced. Left unedited on purpose; ADRs record what was believed when
> they were written.

> **Amended by FR-49 (2026-08-27).** Decision point 6 below says the helper keeps "a coarse
> tool category". It cannot: point 5's refusal to hook pre-tool events leaves five events,
> and none of their payloads carries a tool name. The helper keeps the notification kind
> instead. The decision itself — a helper that does almost nothing — is unchanged. Left
> unedited on purpose; ADRs record what was believed when they were written.

## Context

[ADR-0001](0001-observe-local-state-files.md) made hooks an *optional accuracy layer*: the
board works with zero configuration, and hooks make the *waiting for you* status exact
instead of inferred. That decision left one thing unanswered — **how the user turns them
on**.

The relevant facts:

- The status that hooks fix is the one the whole product exists for. In inference mode it
  is a debounced guess that can be up to 45 s late and is shown with reduced confidence
  ([session-state-model.md](../session-state-model.md) §4). Hook mode is not a nicety.
- Turning hooks on by hand means editing a JSON settings file, in a schema the user has no
  reason to know, in a file that other tools also write to.
- **Most people who use Claude Code have never configured a hook**, and have no reason to
  learn. Any design that ends in *"add the following block to your settings file"* will
  simply not be used — and then the product's most important status stays a guess forever,
  for almost everyone who installs it.
- At the same time, a monitoring tool that edits the user's configuration is exactly the
  kind of thing that deserves suspicion. It reads files containing source code and prompts;
  it must not also be a tool that quietly rewrites settings.

So: the setup must be easy enough that it actually happens, and transparent enough that it
deserves to.

## Decision

**The product performs the hook setup itself, as a guided flow that is explained, previewed,
backed up, verified, and reversible in one click.** It is specified in
[hook-setup.md](../hook-setup.md); the load-bearing parts:

1. **Framed by benefit, not mechanism.** The offer is *"make 'waiting for you' exact"*.
   No step requires the user to know what a hook is or to open a file.
2. **Consent, always.** Never installed automatically, never at first run, never during an
   update. A declined offer stays declined.
3. **Show the change before making it**, take a timestamped backup, and show where the
   backup went.
4. **Additive only.** Entries carry a versioned marker; nothing without that marker is ever
   modified, reordered, or removed. Upgrade and removal match on the marker, and removal
   does not depend on the backup.
5. **Minimal, non-interfering event set.** Five events. Nothing is installed on events that
   run before a tool call, because those can block or alter the call. The product must not
   be able to change what a session does.
6. **A helper that does almost nothing**: keeps event kind, session id, timestamp and a
   coarse tool category, discards everything else, writes one line, always exits 0,
   silently, with a timeout configured as a second line of defence.
7. **Verified, not assumed.** Setup waits for a real event before claiming success, and if
   events later stop arriving the product falls back to inference by itself and says so.

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Document the JSON, let the user paste it** | Zero code; the product never touches the user's settings. | The setup then does not happen. It converts the product's most valuable status into a feature only its author will ever enable. |
| **Install hooks automatically at first run** | The best status detection, with no decision fatigue. | A tool that silently edits a settings file it did not create forfeits the trust it needs to also read transcripts. Non-negotiable. |
| **A separate CLI command (`… --install-hooks`)** | Easy to build; familiar to developers. | Puts a terminal step in front of the one user who explicitly does not want to think about this. The board is the UI; setup belongs in it. |
| **Only hook the notification event** | The smallest possible footprint. | Would leave clean session-end unknown, so normal closes could still be misreported as crashes. Five events is already the minimum that closes that gap. |
| **Also hook pre-tool events for richer detail** | Precise per-tool timing; a nicer activity view later. | Those events sit in the path of every tool call and can block or rewrite it. The risk of a monitoring tool degrading the sessions it watches outweighs any detail it would buy. |

## Consequences

**What gets better**

- The exact-detection path is realistically reachable by the person the product is for,
  which is what makes ADR-0001's "optional" honest rather than theoretical.
- Every change to the user's environment is visible, verified, and undoable — so the
  product can ask for this trust without asking for a leap of faith.
- The refusal to hook pre-tool events is a hard structural guarantee: the product is
  incapable of altering a session, not merely careful not to.

**What it costs**

- A meaningful amount of UI for a feature that is, formally, optional: offer, explanation,
  diff preview, backup, verification, diagnosis, removal.
- A second shipped binary (the helper) with its own compatibility surface, plus the path
  management that comes with it (open decision D-5).
- The product now owns a JSON merge that must be additive and idempotent against a file
  other tools also edit — a small amount of code that has to be right, and needs tests
  against hand-mangled files.
- Two status-derivation paths remain live and both need to be maintained and tested.

**When to revisit**

- Claude Code gains a supported way to subscribe to session events without editing settings
  → drop this flow entirely.
- The hook schema turns out to change often enough that the guided setup breaks more often
  than it helps → fall back to inference-only and remove the feature rather than shipping a
  setup that misleads.
