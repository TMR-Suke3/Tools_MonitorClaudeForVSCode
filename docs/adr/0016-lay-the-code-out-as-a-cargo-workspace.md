# ADR-0016: Lay the code out as a Cargo workspace under `crates/`, and keep `tests/fixtures/` at the root

- **Status**: Accepted
- **Date**: 2026-08-26
- **Deciders**: the author
- **Implements**: [ADR-0004](0004-use-tauri-and-rust.md), which chose Rust + Tauri and named
  the three components. This decides where they live

## Context

[ADR-0004](0004-use-tauri-and-rust.md) settled the runtime and split the product into three
parts: an OS-independent core, the board, and a hook helper that has to start and exit in
milliseconds. Scaffolding them exposed a contradiction that had been harmless while there
was no code.

`CLAUDE.md` and `README.md` both described the layout as:

```
src/       implementation
tests/     tests
```

That is a **single-crate** shape. Three crates cannot share one `src/`, and Cargo gives each
crate its own `src/` and `tests/` whether or not a document says otherwise. Something had to
give, and leaving the documents describing a layout the repository does not have is the one
outcome ruled out in advance: `CLAUDE.md` says a change that touches the requirements does
not get to fix the code and leave the docs behind.

A second question came with it. `tests/fixtures/` already holds **19 committed fixtures**,
cut from real recordings and anonymised ([test-plan.md](../test-plan.md) §3). They are the
input to the core's tests — but they are *recorded data*, not Rust, and more than one crate
will read them.

## Decision

**The three crates live under `crates/`. `tests/fixtures/` stays at the repository root.**

```
Cargo.toml            the workspace
crates/core/          mcv-core   — observation, parsing, state machine, title fitting
crates/board/         mcv-board  — the board (Tauri, once the UI lands)
crates/hook/          mcv-hook   — the hook helper
tests/fixtures/       recorded input, shared by every crate
docs/                 documentation, ADRs, images
```

Three things follow from it, and they are the parts worth stating:

1. **There are two kinds of `tests/` and they do not mean the same thing.**
   `tests/fixtures/` at the root is *data* — the recordings, which belong to no single
   crate and are cited by requirement id from [test-plan.md](../test-plan.md).
   `crates/<name>/tests/` is Cargo's integration-test directory — *code*, which reads that
   data. Moving the fixtures under a crate would have made them look like one crate's
   private files and broken the relative links in `tests/fixtures/README.md` for no gain.

2. **Package names carry the `mcv-` prefix; no crate is called `core`.** A package named
   `core` collides with the Rust built-in of that name, which is a needless trap for a
   library every other crate depends on. The prefix is not invented here — it is already
   the product's short name in [hook-setup.md](../hook-setup.md) §3.2, which specifies the
   helper as `mcv-hook.exe` behind the marker `monitor-claude-vscode v1`.

3. **`crates/board` does not depend on Tauri yet.** The UI is deliberately the *last* thing
   built: the state machine comes first, so that a problem in the Win32 layering costs a
   bounded amount (the order is the one in the [roadmap](../../README.md#roadmap)). A
   `tauri.conf.json` and a placeholder front end written now would be replaced wholesale by
   the real design, and would slow every CI run in the meantime. Tauri arrives with `feat/board-ui`, which is where the HTML the
   design pass produced actually gets used. Until then the binary prints the palette, which
   is a real use of the core and a way to check §3.2 without a window.

## Alternatives considered

| Option | Good | Rejected because |
|---|---|---|
| Keep the documented `src/` + `tests/`, one crate, modules instead of crates | No documents to change; the smallest possible tree | It cannot express the split ADR-0004 made. "The core contains no platform code" stops being structural and becomes a convention — the thing ADR-0004 explicitly bought by separating them. The helper would also lose its own tiny binary |
| `crates/` for core and hook, with the Tauri app at the root (`src-tauri/`, the Tauri default) | Matches what `create-tauri-app` generates, so tutorials line up | Puts one of three peers in a different place for no reason other than a generator's habit, and leaves the root holding a crate again. The Tauri layout is a convention, not a requirement |
| Move `tests/fixtures/` into `crates/core/tests/fixtures/` | Everything the core test needs sits next to it | The fixtures are already committed, already documented at that path, and are not the core's private property: the board will replay them, and the hook helper's tests will want the registry. It also silently rewrites the corpus's provenance links |
| Scaffold the full Tauri app now | Proves the whole toolchain end to end, including WebView2 | The UI is out of scope for this branch, and none of what would be written survives the design being implemented. The toolchain question is real but is answered in `feat/board-ui`, at the moment it can be answered with something that stays |

## Consequences

**Better**

- NFR-10 becomes structural rather than aspirational: `mcv-core` has **no dependencies at
  all**, so it *cannot* reach a Windows API or a UI toolkit by accident. A pull request
  that tries shows up as a new line in `crates/core/Cargo.toml`.
- One `cargo test` at the root runs everything, and `cargo run -p mcv-board` /
  `-p mcv-hook` name the binaries the product ships.
- The documents and the tree now say the same thing. `CLAUDE.md` and `README.md` are
  updated in the same commit as the layout they describe.

**Costs and risks accepted**

- **`CLAUDE.md` and `README.md` had to change**, and any older note that says
  "implementation goes in `src/`" is now wrong. That is the price of having documented a
  layout before writing a line of code.
- **The Tauri toolchain is unproven in this repository** until `feat/board-ui`. If WebView2,
  the Tauri build, or the Windows layering turns out to be a problem, it is discovered later
  than it could have been. That is accepted deliberately: ADR-0004 already records that the
  Win32 work is the risky part, and the plan is to meet it with a working core rather than
  with an empty window.
- **A fixture path now crosses a crate boundary** — a test in `crates/core/tests/` reaches
  `../../../tests/fixtures/`. It wants a single helper that resolves the path from
  `CARGO_MANIFEST_DIR`, written once when the first fixture test lands, rather than the
  same `..` chain copied into every file.

**Triggers to revisit**

- A fourth component appearing that is neither core, board, nor helper.
- The fixtures growing to the point where they want their own repository or Git LFS — at
  which point the root position is what makes the move easy.
