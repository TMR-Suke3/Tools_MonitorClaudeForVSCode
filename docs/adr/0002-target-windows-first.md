# ADR-0002: Target Windows first, keep the observation core OS-independent

- **Status**: Accepted
- **Date**: 2026-08-24
- **Deciders**: Repository owner (@TMR-Suke3)

## Context

The board is an always-on-top, frameless, freely positioned window that must sit above
maximised editors, never steal focus, survive multi-monitor and mixed-DPI setups, and
optionally raise another application's window on click (FR-26 … FR-33).

Every one of those behaviours is where desktop platforms differ most:

- Always-on-top layering, click-through, and per-monitor DPI have different semantics on
  Windows, macOS and Linux — and on Linux, different semantics per compositor (X11 vs
  Wayland, where "put my window on top and move it there" may not be permitted at all).
- Raising another process's window is a platform API, and on Wayland it is largely not
  available.
- Start-on-login and tray behaviour differ per platform.

Against that, the parts that read Claude Code's state — locating the state directory,
tailing JSON-Lines transcripts, deriving status — are ordinary file and string work with no
platform content beyond path handling.

The developer this is being built for works on Windows 11, and the project is at the stage
where a working prototype matters more than reach.

## Decision

**Ship Windows only for the first release, and keep the observation and status logic in an
OS-independent core that the UI layer sits on top of.**

- Platform-specific code is confined to the window/UI layer and to process liveness and
  window activation.
- The core — source discovery, tailing, parsing, the state machine, title fitting — takes
  paths and events as input and contains no platform branches (NFR-10).
- The core's tests run from recorded fixtures, so they are portable from the start
  (NFR-11).
- README and documentation state plainly that only Windows is supported; no claim is made
  about other platforms until one is actually tested.

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Cross-platform from day one** | Wider audience for a public repository; avoids a later port. | Triples the surface where the hardest requirements live (layering, DPI, window activation), on platforms that cannot be tested here. Would delay a working prototype for reach nobody has asked for yet. |
| **Windows only, no separation** | Slightly less structure to maintain. | Makes a later port a rewrite, and — worse — makes the status logic untestable without a desktop session. The separation pays for itself in tests alone. |
| **Web UI in a browser tab** | Trivially portable. | A browser tab cannot be always-on-top, frameless, freely positioned, or focus-safe. It fails the core requirement of the product. |

## Consequences

**What gets better**

- The hard platform work is done once, well, on the one platform that can actually be
  tested.
- The status model — the part of this project with real design content — is testable
  headlessly and portable by construction.
- The scope of the first release is honest and small.

**What it costs**

- Non-Windows users get nothing at first, and the repository must say so rather than imply
  a cross-platform tool.
- A discipline cost: every "just call the Win32 API from here" shortcut in the core has to
  be resisted, or the portability claim quietly becomes false.

**When to revisit**

- The first release is stable on Windows and someone actually wants macOS or Linux.
- The runtime chosen for the UI (still an open decision) turns out to make another platform
  nearly free — in which case reconsider, but only with a machine available to test on.
