# Tools_MonitorClaudeForVSCode

> An always-on-top status board for all your Claude Code for VS Code sessions.

[![CI](https://github.com/TMR-Suke3/Tools_MonitorClaudeForVSCode/actions/workflows/ci.yml/badge.svg)](https://github.com/TMR-Suke3/Tools_MonitorClaudeForVSCode/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> **Status: usable, and rough at the edges.** The board runs, watches the sessions running
> right now, and clicking one raises its window. What is missing is listed under
> [Limits](#limits) — none of it stops you using the board today.
>
> Every picture below is drawn from the same palette and geometry the board uses, but it is
> a design specification rather than a screenshot.

---

## Why

Developers routinely run several VS Code windows, each on a different repository, and often
more than one Claude Code session inside a single window. VS Code cannot show all of those
sessions at once — so a session that finished, crashed, or is waiting for a permission
answer stays silently blocked until you happen to look at it.

The cost is the manual polling: cycling through windows to find out who is idle. This tool
removes it by answering one question at a glance:

> **Which of my sessions needs me right now?**

## Features

![The board — design specification, not a screenshot](docs/images/board-mixed.svg)

- A small always-on-top window, placed anywhere you like, in front of any editor
- Workspaces fold to one row each, so twelve sessions fit in 300 × 90 px
- One coloured indicator per live session, grouped by workspace folder
- Five statuses: working · waiting for you · stopped · terminated abnormally ·
  usage limit reached
- Click a session to bring its VS Code window to the front — and the board says what
  happened either way, rather than failing silently
- A tray icon in the board's roll-up colour, so a board put away still answers
  "is anyone waiting for me?"
- Two global shortcuts — fetch the board to the display the mouse is on, and show / hide it —
  and both can be changed or cleared, because the key this picked may already be somebody else's
- One board per machine: starting it again fetches the board you already have
- Session titles in whatever language the conversation uses
- Local: no network traffic, no stored conversation content. Observation is strictly
  read-only; the only thing ever written is the hook setup below, if you ask for it
- Works with zero configuration — and sets itself up for exact detection, in one click,
  without you editing anything ([how](docs/hook-setup.md))

## Requirements

- Windows 11 — see [ADR-0002](docs/adr/0002-target-windows-first.md)
- Visual Studio Code with the Claude Code extension
- Built with Rust + Tauri — see [ADR-0004](docs/adr/0004-use-tauri-and-rust.md)

## Installation

From the [latest release][releases], take whichever suits you. They differ only in
packaging.

- **`…-setup.exe`** — the ordinary installer. No administrator needed; it installs for your
  account only, and nothing is written outside your user profile.
- **`…_en-US.msi`** — the same thing as an MSI, for machines that are centrally managed.
- **`…-windows-x64.zip`** — unpack it anywhere and run `mcv-board.exe`. **Keep
  `mcv-hook.exe` in the same folder**: the board looks for the helper beside its own
  executable, and the optional exact-mode setup below cannot install without it. The
  installers take care of this themselves.

Windows will warn that the publisher is unknown. These are not code-signed.

Every download has a `.sha256` beside it, which tells a good one from a damaged one:

```powershell
(Get-FileHash .\mcv-board-*-windows-x64.zip -Algorithm SHA256).Hash.ToLower()
```

[releases]: https://github.com/TMR-Suke3/Tools_MonitorClaudeForVSCode/releases/latest

### From source

Rust 1.85 or newer is the only build dependency; the workspace pulls in nothing else you
have to install first.

```bash
git clone https://github.com/TMR-Suke3/Tools_MonitorClaudeForVSCode.git
cd Tools_MonitorClaudeForVSCode
cargo build --release -p mcv-board -p mcv-hook
```

Both packages, for the reason above: `-p mcv-board` alone leaves no `mcv-hook.exe` beside
the board, and exact mode then has nothing to install. The two land in `target/release/`.

## Usage

Run `mcv-board.exe` with no arguments and it watches the sessions running right now. There
is nothing to configure first.

- **Right-click the board** for the menu: the mode it is in, the size to draw it at
  (100 – 200 %, remembered), the two shortcuts, the hook setup, the diagnosis, and quit.
  The tray icon carries the same menu; left-clicking it brings the board to the display the
  pointer is on, or takes it away when it is already in front of you
- **Click a session row** to raise the VS Code window that owns it. A ring means it worked;
  an outline and a reason mean it did not
- **`Ctrl+Alt+M`** brings the board to the display your mouse is on and flashes its edge —
  the answer to losing it across monitors. **`Ctrl+Alt+B`** shows or hides it. Neither takes
  focus away from what you are doing
- **Exact mode** is opt-in and takes one click, from the menu's hook setup. It shows you the
  exact change before writing anything, backs the file up first, and removes itself in one
  action ([what it changes](docs/hook-setup.md))

### Recommended setup

Three things, none of them on until you ask for them, that make the board the one it was
designed to be. The first two are a click each in the menu; the third you place yourself.

**Notice waiting instantly…** is exact mode, above. Inferred, "waiting for you" has to be
worked out — a tool call the transcript says is pending, with no process running for it —
which costs seconds, and longer for the tools that never spawn a process at all. Installed,
the board is *told*, as it happens.

**Focus the session tab on click…** makes a click on a row bring up that session's own tab,
instead of only the window it lives in. In a window holding several sessions, raising the
window alone is half the job.

They are separate switches: turning one on leaves the other where it was.

**Start it with Windows.** There is no start-on-login switch in the menu — it writes to your
`Run` key, which deserves the same show-it-first, undo-in-one-action flow the hooks get, and
that is not built. Put a shortcut in the Startup folder instead: press `Win`+`R`, run
`shell:startup`, and drop a shortcut to `mcv-board.exe` into the folder that opens. Or do the
same thing in one go:

```powershell
$startup = [Environment]::GetFolderPath('Startup')
$link = (New-Object -ComObject WScript.Shell).CreateShortcut("$startup\Monitor Claude.lnk")
$link.TargetPath = "C:\path\to\mcv-board.exe"
$link.Save()
```

Deleting that shortcut undoes it. This matters more than it sounds: the board is only a
glance-and-know if it is already there when you look, and a monitor you have to remember to
start is one you check by remembering instead.

If clicking a row does nothing, `--check-raise` walks the whole session-to-window chain and
prints each step, which is usually enough to see why. The menu's **diagnosis** does the same
for exact mode: one line per precondition, each with the single action that repairs it.

<details>
<summary>Other options — mostly for looking at the thing</summary>

```
--demo          drawn from the recordings instead of what is running
--light         the light theme
--still         no animation: a stable picture, and the reduced-motion rendering
--zoom <n>      draw everything at n times size, to inspect a silhouette
--setup         also open the hook setup window
--diagnose      ...open it on the diagnosis panel instead
--reveal-setup  ...or on the session-reveal explanation
--palette       print the palette and its measurements
--tray-icons    print every tray icon as pixels
--check-raise   walk the session-to-window chain and print each step
```

</details>

## Limits

What this is, so you know before you install it:

- **Windows 11 only** — [ADR-0002](docs/adr/0002-target-windows-first.md)
- This reads Claude Code's own files, which are internal formats with no stability promise.
  Every one of them is parsed defensively, but **a Claude Code update can still break
  detection**; see [docs/observation-sources.md](docs/observation-sources.md) for exactly
  what is read and what it cannot show

## Development

Requires **Rust 1.85 or newer** — the floor for edition 2024, declared as `rust-version` in
the root `Cargo.toml`, so Cargo says so plainly if your toolchain is older. There are no
other build dependencies: the core crate has none at all. CI builds on current stable only,
so older-but-supported toolchains are not exercised.

```bash
cargo test --workspace                                 # includes the palette suite below
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p mcv-board                                 # the board, watching what is live
```

Directory layout — a Cargo workspace of three crates
([ADR-0016](docs/adr/0016-lay-the-code-out-as-a-cargo-workspace.md)):

```
crates/core/      mcv-core  — observation, parsing, state machine, title fitting.
                  A pure function of what it is given: no clock, no filesystem,
                  no process table, so it tests headlessly (ADR-0019)
crates/board/     mcv-board — the board, and everything that touches the OS:
                  following files, probing processes. Tauri lands with the UI
crates/hook/      mcv-hook  — the hook helper
tests/fixtures/   recorded input, shared by every crate
docs/             public documentation (design, how to use)
docs/adr/         architecture decision records — one decision per file
```

**What is implemented:** the palette, the observation core — finding live sessions,
following their transcripts by byte offset, reading the records defensively — and the
**status state machine**: the rules in
[docs/session-state-model.md](docs/session-state-model.md) that decide whether a session is
working, waiting for you, stopped, cut off by the usage limit, or gone. Around 240 tests
([docs/test-plan.md](docs/test-plan.md)); most are replays of the recordings in
`tests/fixtures/`, and the ones that cannot be — the editor's lock files carry an auth token
and are deliberately kept out of the corpus — say so in their own module.

**The board runs.** `cargo run -p mcv-board` opens an always-on-top window that watches the
sessions running right now: it finds them in Claude Code's registry, follows their
transcripts, derives each one's status, and draws them grouped by workspace. `--demo` draws
a fixed board from the recordings instead, which is what the screenshots are of.

**Clicking a row** raises the VS Code window that owns that session, and the board says
what happened either way — a ring when it worked, an outline and a reason when it did not.
`--check-raise` walks that chain and prints every step, for when it does not.

**Hook setup** is reachable by right-clicking the board: it explains what would change,
shows the exact diff of the five entries before writing anything, takes a timestamped backup,
and removes them again in one action. `mcv-hook` records the events.

**Exact mode works.** With the entries installed, the board is *told* when Claude asks you
something instead of working it out from a pending tool call — and told when a turn ends and
when a session closes cleanly. Where a hook reports a transition, the inference for it is
skipped entirely. If the reports stop arriving, the board falls back to inference on its own
and says so, rather than claiming an exactness it is no longer earning.

**The board has a menu.** Right-click it: the mode it is in, the size to draw it at (100 to
200 %, remembered), what the two shortcuts are — and whether they registered — the hook setup,
and quit.

**Two keys work while something else has focus.** `Ctrl+Alt+M` brings the board to the display
the mouse is on and flashes its edge, which is the only real answer to losing it across
monitors. `Ctrl+Alt+B` shows or hides it. Neither takes focus. **Both can be changed or
cleared** from the menu — a key this product picked can already belong to something else on
your machine, and what the menu reports for each one is what the operating system answered
when it was asked for, not what was intended.

**A tray icon carries the same menu.** A left-click brings the board to the display the pointer
is on, and takes it away only when it is already in front of you — a board parked on a display
that has since been unplugged used to be "shown" exactly where nobody could look. The icon is
drawn in the board's roll-up colour, so a board put away with `Ctrl+Alt+B` still answers "is
anyone waiting for me?". It is the board in miniature rather than a coloured dot, because the
palette's separations are measured against the board's own background and nowhere else.

**The menu also opens a diagnosis**, which reports each precondition of exact mode with the
single action that repairs it, and — as an opt-in, off by default — can bring the clicked
session's own tab to the front rather than only the window it is in.

**What is not there yet:** fold all / unfold all, opacity, click-through, hide titles, the
optional notification of FR-34, and start-on-login.

Documentation:

| Document | What it covers |
|---|---|
| [docs/requirements.md](docs/requirements.md) | Scope, functional and non-functional requirements, acceptance |
| [docs/session-state-model.md](docs/session-state-model.md) | The statuses, what they mean, and when they change |
| [docs/observation-sources.md](docs/observation-sources.md) | Which local files are read, and the limits of what they show |
| [docs/test-plan.md](docs/test-plan.md) | How every requirement gets verified, and which ones cannot be |
| [docs/ui-overlay.md](docs/ui-overlay.md) | The visual specification: geometry, palette, motion, interaction |
| [docs/hook-setup.md](docs/hook-setup.md) | The one-click setup for exact status detection |
| [docs/adr/](docs/adr/) | Why the key decisions were made |

## Contributing

**Issues are welcome** — bug reports, and anything the board got wrong about a session,
are genuinely useful. See [CONTRIBUTING.md](CONTRIBUTING.md) for what makes a report easy
to act on.

**Code contributions are not accepted.** This is a personal project and its design is not
open to change, so pull requests are turned off rather than left open and refused.

For security problems, please follow [SECURITY.md](SECURITY.md) and report privately
instead of opening an issue.

## License

MIT © 2026 TMR-Suke3
