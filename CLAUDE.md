# CLAUDE.md

Guide for Claude Code / AI agents working in this repository.

## What this project is

A tool that gathers the state of every Claude Code for VS Code session — across several
VS Code windows and several sessions inside one window — into one small always-on-top
window (the *board*). One coloured indicator per session, so the answer to "which session
needs me right now?" is available at a glance.

The requirements and the design are settled and live in `docs/`. The implementation is
**Rust + Tauri** (ADR-0004), but the project has not been scaffolded yet.

Read in this order:

1. `docs/requirements.md` — scope and requirements (every FR / NFR has an id)
2. `docs/session-state-model.md` — the statuses and their transitions. **The core design**
3. `docs/observation-sources.md` — which files are read, and the limits of what they show
4. `docs/ui-overlay.md` — layout and interaction
5. `docs/hook-setup.md` — the guided hook setup (optional, makes detection exact)
6. `docs/adr/` — why the key decisions were made

**When a change touches the requirements, do not fix the code and leave the docs behind.**
Changing a requirement means editing the relevant FR / NFR in `requirements.md`; changing a
decision means writing a new ADR (never rewrite an existing one — mark it Superseded).

## Project structure

A Cargo workspace of three crates (`docs/adr/0016-lay-the-code-out-as-a-cargo-workspace.md`).

```
Cargo.toml           the workspace
crates/core/         mcv-core  — observation, parsing, state machine, title fitting.
                     No OS-specific and no UI code (NFR-10); no dependencies
crates/board/        mcv-board — the board. Tauri arrives with feat/board-ui; Windows
                     specifics stay isolated in a thin Windows module
crates/hook/         mcv-hook  — the hook helper (docs/hook-setup.md)
tests/fixtures/      recorded input, shared by every crate — data, not code
docs/                public documentation (requirements, design, usage)
docs/adr/            architecture decision records — one decision per file
docs/images/         screenshots / GIFs for the README
.notes/              working notes (gitignored — never committed)
.notes/plans/        implementation plans, one file per feature
.notes/plans/_done/  plans whose feature has shipped
.notes/rules/        personal working rules (gitignored — not public)
```

**Two different `tests/`.** `tests/fixtures/` at the root is recorded *data*, cited by
requirement id from `docs/test-plan.md`. `crates/<name>/tests/` is Cargo's integration-test
directory — the *code* that reads it.

## Commands

Rust **1.85 or newer**, edition 2024 (`rust-version` in the root `Cargo.toml`; CI runs
current stable). Every command below runs from the repository root, and every one of them is
what CI runs (`.github/workflows/ci.yml`).

```bash
cargo test --workspace                              # all tests, including TC-48 … TC-54
cargo test -p mcv-core --test palette               # just the palette suite
cargo fmt --all --check                             # formatting, as CI checks it
cargo fmt --all                                     # ...and applying it
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo run -p mcv-board                              # prints the palette; no window yet
```

**Do not add a command here until it runs.** A command that does not work is worse than an
absent one, because it is believed.

What is implemented so far: the palette, discovery, tailing, the record parser and the state
machine in `mcv-core`; the board's window, the click-to-raise chain, the hook setup flow and
the helper in `mcv-board` / `mcv-hook`; hook mode — where the hook layer reports a
transition, the inference for it is skipped
(`docs/adr/0020-hook-signals-are-facts-not-status-writes.md`); and the board's menu, its two
global shortcuts and the tray icon that carries the same menu
(`docs/adr/0025-the-tray-icon-is-the-board-in-miniature.md`); and keeping the window
findable — one corner for every summons, a rescue that runs while the board runs, and a tray
menu that says where the board is
(`docs/adr/0031-one-corner-for-every-summons.md`); and one board per machine, where a second
launch is a summons rather than a second board
(`docs/adr/0032-one-board-per-machine.md`); and shortcuts that can be changed or cleared, with
what became of each key measured rather than assumed
(`docs/adr/0033-a-key-that-is-taken-is-a-choice-to-undo.md`).

The setup window now holds four screens: the install flow (`docs/hook-setup.md` §4), the
diagnosis (§5, FR-55), which reports each precondition of hook mode with the single action
that repairs it (`docs/adr/0026-the-diagnosis-lives-in-the-setup-window.md`), the
session-reveal explanation, and the one that changes the two global shortcuts (FR-61).

Still to come: `ui-overlay.md` §6.2's remaining menu items — fold all / unfold all, opacity,
click-through, hide titles, and the notification toggle of FR-34 — and start-on-login, which
is FR-32's other half and stays deferred because it writes to the user's `Run` key.

## Conventions

- Commits: Conventional Commits (`feat:` `fix:` `docs:` `refactor:` `test:` `chore:`
  `perf:` `ci:`). Subject in English, lowercase, imperative, 50 characters or fewer
- Branches: `<type>/<issue-number>-<short-description>` (lowercase, English,
  hyphen-separated)
- Documentation language: **everything that is committed is written in English** — README,
  `docs/`, and this file. Files that are gitignored (`.notes/`, `CLAUDE.local.md`) may be
  in Japanese
- **No translated copies for now.** Do not add `*.ja.md` (or any other translation)
  alongside a committed document: the specs still change too often, and a stale translation
  is worse than none. If translations are introduced later, they need a stated normative
  language and an automated drift check — not just a copy
- Tests: new logic always ships with tests

## Where implementation plans go — non-negotiable

When starting a new feature or fix, **always create the implementation plan under
`.notes/plans/`, one file per feature.** Never create `plan.md` / `TODO.md` /
`実装方針.md` in the repository root or in `docs/`.

**File name** (matches the branch name, without the `feat/`-style type prefix):

```
.notes/plans/<issue-number>-<short-english-description>.md
```

| Branch | Plan file |
|---|---|
| `feat/12-add-oauth-login` | `.notes/plans/12-add-oauth-login.md` |
| `fix/34-crash-on-empty-input` | `.notes/plans/34-crash-on-empty-input.md` |

- Work without an issue number uses the date instead:
  `.notes/plans/2026-08-24-add-activity-log.md`
- English, lowercase, hyphen-separated (never a Japanese filename)
- **Never overwrite an existing plan file.** A different feature gets a new file
- If the approach changes, update the plan too — never leave it contradicting the code
- Move the plan to `.notes/plans/_done/` once the feature ships
- **`.notes/` is gitignored. Plan files must never be committed**
- The one part of a plan worth keeping — *why this design* — is written up as an ADR in
  `docs/adr/` and committed (`Context / Decision / Consequences`)
- **No secrets and no client information in `.notes/` either.** Gitignore only means "not
  committed"; the files are still visible in screen shares and backups

## Rules for AI agents

- **Never commit or push automatically.** A human reviews every diff
- **Never modify the user's `~/.claude/settings.json` on your own.** Installing hooks is a
  product feature that happens only with the user's explicit consent
  (`docs/hook-setup.md`). Hooks added by hand while testing must always be removed again
- Never put secrets (keys, tokens, personal data) in code or documentation
- Never write absolute paths (`C:\Users\...`); use repository-relative paths
- Working notes and plans belong in `.notes/` (plans in `.notes/plans/`). `docs/` holds only
  ADRs and documentation written for a reader
- When adding a dependency, state the reason in the PR description
- Match the existing code style; discuss large refactors before starting
- Never describe an unimplemented feature in the README as if it worked

## Local-only notes

If `.notes/rules/` or `CLAUDE.local.md` exists in this working copy, read it before making
changes. Those files are git-ignored and are not part of the public repository.

## Gotchas

Verified on a real machine, about the files this product observes. Details in
`docs/observation-sources.md`.

- **`~/.claude/ide/*.lock` files are never cleaned up.** Measured: 3 live of 84. Always
  validate the process before using one
- **`~/.claude/sessions/*.json` is swept, but only when some Claude Code process exits
  cleanly** — and then it sweeps *every* dead entry at once, not just its own (measured
  twice: 17 entries collapsed to the 3 live ones). So a snapshot can be mostly stale, and
  entries can vanish in bulk for reasons unrelated to any session ending. Validate
  liveness, and never read a disappearance as "that session just died" without checking.
  The `sessions/*.key` siblings are **not** swept
- **A killed process leaves its `sessions/<pid>.json` behind; a clean exit removes it.**
  That difference is the only local evidence distinguishing the two (measured directly)
- **`entrypoint` and `kind` are inherited from the environment, not derived from how the
  process was started.** A `claude -p` run launched from inside a VS Code session's shell
  registers as `entrypoint: claude-vscode`, `kind: interactive`. The FR-02 filter can be
  fooled this way
- **Append order is not timestamp order.** 56 of 78 transcripts contained records written
  out of order (940 in total); `api_error` is always appended after the turn it belongs to
  has ended, carrying its original earlier timestamp. Order by byte offset, never by
  `timestamp` — several record types have no `timestamp` field at all
- **The directory names under `~/.claude/projects/` are a lossy encoding of `cwd`** (both
  `_` and `-` become `-`). Never decode them back; take the path from the `cwd` field of the
  records
- **PID alone does not identify a process.** PIDs get reused — also compare the process
  creation time
- **A VS Code extension host listens on several ports, and only one has a `ide/*.lock`.**
  Measured: 54851, 58915, 59126 for one host, lock under 59126 only. Never take the first
  port off the process table — offer every port to the lock files and let them answer
  (`docs/adr/0027-pick-the-editors-port-by-its-lock-file.md`). Getting this wrong makes
  clicking a session a silent no-op, which looks exactly like a board ignoring the mouse
- **`workspaceFolders` in a lock file can legitimately be empty** — an editor window with no
  folder open. There is then nothing to match in its title, and the raise correctly fails
- **The VS Code extension's URI handler is documented, but absent from its `package.json`.**
  `…/open?session=<id>` reveals that session's tab and focuses its message box, with no auth
  token involved (code.claude.com/docs/en/vs-code, *Launch a VS Code tab from other tools*);
  the board's opt-in session reveal rides on it
  (`docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md`). Two things bite.
  **The editor picks which of its windows answers**, and a session sent to a window whose
  workspace it does not belong to opens a *duplicate* there rather than doing nothing — so the
  session's `cwd` has to be checked against that window's `workspaceFolders` first. And **the
  shell's URL association does not work**: it runs `Code.exe --open-url`, which that
  executable rejects outright; only `bin/*.cmd` does, because it hands the arguments to
  `cli.js`
- **The opening prompt is not the first text block.** The editor puts the file on screen, or
  the selection, in a text block of its *own* ahead of the one the user wrote. Measured: 41
  of 69 transcripts open that way (`<ide_opened_file>` 38, `<ide_selection>` 3), so reading
  the first text block names the majority of sessions after the editor's note. Skip a block
  that is nothing but complete `<tag>…</tag>` elements — the shape, not a list of tag names,
  which the next tag would break
- **A transcript grows past several hundred KB within one session.** Never re-read it from
  the start; tail it by byte offset
- **`GetWindowRect` and Tauri's own coordinates differ by the invisible DWM frame.** Measured
  8 px on this machine: a board placed by Tauri at 1510, 40 reads back from Win32 as 1518, 32.
  Both are self-consistent — Tauri's positions, sizes and work areas agree with each other,
  and so do Win32's. Never compare one to the other and conclude the placement is wrong
- **Lock files contain an auth token.** Do not read it, store it, or log it
- Every format read here is a Claude Code internal. Assume it will break on an update and
  parse defensively
