# ADR-0004: Build the board with Rust and Tauri

- **Status**: Accepted
- **Date**: 2026-08-24
- **Deciders**: Repository owner (@TMR-Suke3)

## Context

This was open decision D-1 — the last thing blocking a prototype. The requirements that
constrain the choice:

- **A real desktop window.** Always-on-top over maximised editors, frameless, freely
  positioned, focus-safe, adjustable opacity, optional click-through, tray icon, correct
  under mixed-DPI multi-monitor setups (FR-26 … FR-32). A browser tab cannot do any of
  this.
- **A resident process with a small footprint.** The board runs all day next to the editors
  it watches: below 1 % CPU idle and below 150 MB RSS (NFR-03). A monitoring tool that costs
  more than what it monitors is self-defeating.
- **Cheap incremental file watching.** Tailing several append-only transcripts by byte
  offset, continuously (NFR-02).
- **An OS-independent core.** Discovery, parsing, the state machine and title fitting
  must contain no platform code, and must be testable headlessly from fixtures
  ([ADR-0002](0002-target-windows-first.md), NFR-10, NFR-11).
- **Some Win32 work is unavoidable** regardless of toolkit: window layering, click-through,
  and raising another process's window for FR-33.
- **A tiny, fast helper binary.** The hook helper runs on every session event and must start
  and exit in milliseconds, always successfully ([ADR-0003](0003-guided-hook-setup.md),
  FR-48). Anything with a runtime to boot is disqualified for this job.
- **The visual design will arrive as HTML/CSS.** The design pass (D-2) produces markup, and
  reusing it directly is worth real time.

## Decision

**Rust for the core and the helper; Tauri for the window and the UI.**

- The core (source discovery, tailing, parsing, state machine, title fitting) is a plain
  Rust library with no platform or UI dependencies, tested from recorded fixtures.
- The board is a Tauri application rendering HTML/CSS through WebView2, which ships with
  Windows 11.
- Platform specifics — layering, click-through, foreground activation, process liveness —
  are isolated behind a thin Windows module in the app layer.
- The hook helper is a second small Rust binary from the same workspace, sharing the event
  types with the core.

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Electron** | Everything needed for an overlay is well-trodden; huge ecosystem; direct HTML reuse; `fs.watch` out of the box. | A resident Electron app costs 150–250 MB. It fails NFR-03 on day one, in the one product category where being cheap to keep running is the point. |
| **WPF / WinUI 3 (C#)** | Native Windows; DPI, always-on-top and window activation are first-class; light. | Throws away the HTML design output, and the natural core language (C#) makes the helper a .NET process with startup cost where milliseconds matter. |
| **Avalonia (C#)** | Keeps a cross-platform door open; one language for core and UI. | Same helper-startup problem, and the Windows-specific overlay behaviours still have to be written by hand — paying the abstraction cost without collecting the benefit. |
| **Python + Qt** | Fastest to a first window. | Heavy, awkward to distribute as a resident desktop app, and the worst fit for the always-running footprint requirement. |
| **Rust with a native GUI crate (egui, etc.)** | No WebView dependency; smallest possible binary. | Discards the HTML design output and puts every pixel of the visual design back on hand-written code. |

## Consequences

**What gets better**

- The footprint requirement is met by construction rather than fought for, and the state
  machine — the part of this project with actual design content — lives in a language with
  the type system to encode it.
- One toolchain produces both binaries, and the helper is a statically-linked executable
  that starts fast enough to be invisible inside a session event.
- The design pass's HTML/CSS is used directly, so visual iteration does not require
  rewriting the UI.
- ADR-0002's portability claim becomes structural: the core cannot accidentally depend on
  Windows, because it does not depend on the UI at all.

**What it costs**

- **Rust, with everything that implies** for someone not already fluent in it. This is the
  real price of the decision and it is being paid deliberately.
- Windows-specific behaviour (click-through, topmost layering, foreground activation) means
  calling Win32 directly rather than getting it from the framework.
- A WebView2 dependency. It is present on Windows 11, but it is one more thing that can be
  broken on a user's machine, and the product should fail legibly if it is.
- Two build artefacts to package and keep in sync (board + helper), which feeds open
  decision D-5 about where the helper lives.

**When to revisit**

- Rust turns out to be the thing that stops the prototype from existing → reconsider WinUI 3
  with the same core/UI split, accepting the loss of the HTML design reuse.
- Cross-platform support becomes real (ADR-0002) → Tauri makes it plausible, but only the
  UI layer's platform code is in question, and that has to be tested on a real machine
  before any claim is made.
