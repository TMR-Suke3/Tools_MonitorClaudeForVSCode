# ADR-0019: The core is a pure function of an observation stream — the clock included

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Implements**: [ADR-0004](0004-use-tauri-and-rust.md) (an OS-independent core) and
  [ADR-0016](0016-lay-the-code-out-as-a-cargo-workspace.md) (where it lives). This decides
  what "OS-independent" is allowed to mean, and updates one claim ADR-0016 made about
  dependencies

## Context

NFR-10 says the core carries no platform code and NFR-11 says it is testable headlessly.
Both are easy to satisfy loosely — no `windows` crate, and fixtures on disk — and the
implementation phase is where "loosely" stops being enough.

The test plan had already closed the loophole. [test-plan.md](../test-plan.md) §5.4, TC-45a:

> Run the whole suite with the clock, the filesystem and the process table made unavailable
> to the core. Every status must still be produced, because time and liveness arrive **as
> events in the sequence**, not as calls. **A core that reads `now()` fails this test.**

That is not a testing convenience. Three of the state model's rules are timing rules —
`T_pending_interactive` 1.5 s, `T_pending_probe` 2 s, and the 45 s that
[ADR-0007](0007-detect-subagents-from-their-own-file.md) had to carve an exemption out of —
and a component that fetches the current time cannot be replayed. It cannot be tested
against a recording without either sleeping through it or lying to it, and neither produces
evidence about the product.

The recordings had already settled the shape, before any of this was written.
`prompt-observed.jsonl` is not a transcript: it is a **merged observation stream**, each line
tagged `transcript` or `process`, recorded across a real permission prompt held open for 44
seconds. It interleaves records with process-table transitions in the order an observer saw
them. The fixture is the design.

## Decision

**`mcv-core` is a pure function of an observation stream. It never reads the clock, the
filesystem or the process table; all three arrive as data.**

- Time is a value ([`time::Timestamp`]), parsed out of records or delivered as a tick. There
  is no `now()` anywhere in the crate, and there is no way to add one without adding a
  dependency or a `std::time` import that review will see.
- Liveness is a value (`session::LiveProcess`). Discovery takes the registry entries the
  outer layer read and the processes it found, and returns what the board should draw. It
  cannot consult a process table it was not handed.
- Reading directories, tailing files by byte offset, and probing processes are the **outer
  layer's** job, in the board crate, where the Windows-specific parts already live.

**One consequence of ADR-0016 is updated by this.** That ADR argued NFR-10 was structural
because `mcv-core` had *no dependencies at all*, so it could not reach a Windows API by
accident. It now has two — `serde` and `serde_json` — and the argument has to be restated
rather than quietly kept: what makes the core OS-independent is that its dependencies are
**format libraries, not platform ones**, and that it takes the ambient world as arguments.
The dependency list is still short enough to read in a diff, and adding a platform crate to
it still shows up as a line in a `Cargo.toml`.

The two were added deliberately. A defensive JSON reader is not a small thing to hand-roll:
it fails on escapes, on lone surrogates, on number formats and on nesting depth, and it
fails there long before any fixture notices. Every format this product reads is a Claude Code
internal that will change under it, so the parser has to be boring.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| A `Clock` trait the core calls, faked in tests | The conventional answer; keeps the call site natural | It makes the core's behaviour depend on the *order* of its own calls rather than on the sequence it was given, so a replay is only as faithful as the fake. TC-45a is stricter on purpose, and it is stricter in the direction that catches real bugs: a status that depends on when the observer happened to look |
| Let the core read files, keep only Win32 out | Much less plumbing; the fixtures are files anyway | "OS-independent" would then mean "no Win32", and NFR-11's headless replay would be a property of the test harness rather than of the design. It also puts the offset bookkeeping in the same crate as the parsing, which is where a re-read from zero becomes easy to introduce (NFR-02, TC-40) |
| Hand-roll the JSON reader to keep the dependency list empty | ADR-0016's argument survives verbatim | It buys a slogan and pays for it in the one place this product cannot afford to be clever. The honest fix was to restate the argument |
| Put the outer layer in `mcv-core` behind a feature flag | One crate; the split is still expressible | A feature flag that is on in every real build is not a boundary. The compiler enforces a crate boundary and does not enforce a habit |

## Consequences

**Better**

- Every core test is a replay. `tests/discovery.rs` states which processes are running and
  gets a deterministic answer; `tests/records.rs` replays each recording twice and compares.
  Neither needs a sleep, a temp directory or a real session.
- Liveness being data is what makes the discovery cases writable at all. The recording has
  three live sessions in three different folders, but FR-05 and FR-07 are about two live
  sessions in *one* folder — a case the test can simply declare, because the arrangement is
  an input rather than a property of the recording.
- The timing rules become testable before they are written. `feat/state-model` can drive
  `T_pending_probe` by putting a tick in the sequence.

**Costs and risks accepted**

- **More plumbing.** Everything the core needs must be carried to it, and the outer layer
  grows a job — watching, offsets, process probing — that a less disciplined design would
  have spread thinly through the parser.
- **The outer layer is where the untested code now lives.** Purity in the core does not make
  the file watcher correct; it concentrates the part that can only be tested against a real
  filesystem, and TC-40 / TC-41 have to be honest about testing it there.
- **ADR-0016's dependency claim no longer reads as written.** It is corrected here rather
  than edited there, per the rule that an ADR records a decision as it was made.

**Triggers to revisit**

- A requirement that genuinely needs the core to act rather than to derive — an alert that
  must fire on a schedule, say. A tick can express it, but if the ticks start driving
  behaviour that has nothing to do with the observed sequence, the boundary is in the wrong
  place.
- The dependency list growing past what a reviewer will actually read.

[`time::Timestamp`]: ../../crates/core/src/time.rs
