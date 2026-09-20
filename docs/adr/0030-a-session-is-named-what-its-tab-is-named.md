# ADR-0030 — A session is named what its tab is named

- **Status**: Accepted
- **Date**: 2026-08-28
- **Requirement**: FR-19, NFR-07

## Context

The board named sessions `notes-e8` and `tools-monitorclaudeforvscode-92` while the editor's
own tabs said **メモを整理して** and **Tools_MonitorClaudeForVSCode トレイアイコンと診断パネル**.
The same session, two different names, six inches apart on one screen. Whatever the board's
name was derived from, a user reading both has to translate between them — and the board's
whole argument is that it answers a question at a glance.

Measured on a live machine, across the four sessions running at the time:

| Source | Where | Present |
|---|---|---|
| `ai-title` records | the transcript, `{"type":"ai-title","aiTitle":…}` | 1 session of 4, 126 records of it |
| `name` + `nameSource` | `sessions/<pid>.json` | 4 of 4, **every one `derived`** |
| the opening human prompt | the transcript's first `user` record with `origin.kind: "human"` | 4 of 4 |

And the tabs: the session with an `ai-title` was named after it; the ones without were named
after their opening prompt. So the editor's rule is **the generated title once there is one,
and the opening prompt until then** — and `name` is a slug the CLI makes from the folder and
the id, which the editor never shows at all.

The board had that ordering backwards: `ai_title`, then `registry_name`, then a `prompt_line`
slot that was always passed `None`. The slot existed; nothing filled it.

**Filling it is not free.** `record.rs` is an allow-list, and its own comment says so:

> Anything not named there is never materialised — which is how `queue-operation.content`
> (a full prompt) … are dropped *unread* rather than dropped later (NFR-07, TC-43). Adding a
> field to `Raw` is therefore a decision about what this product is allowed to see, and
> belongs in review.

Reading the opening prompt means reading what the user wrote. NFR-07 permits it — "title" is
in its own list of what may be kept for the lifetime of the session — but the parser's
stricter promise, that prompt text is never built at all, would stop being true.

## Decision

**Name a session the way the editor names its tab**, and pay for it in the narrowest way
available.

The order becomes: a name **somebody chose** → the **generated title** → the **opening
prompt** → the **derived slug** → the workspace label.

- `nameSource` splits `name` in two. `derived` is a slug and ranks below the prompt; anything
  else was chosen by a person and ranks above what the board can work out. The slug stays
  above the workspace label because it still tells two sessions of one workspace apart.
- **The order of a chosen name against the generated title is not measured.** Every
  `nameSource` on the machine was `derived`, so no recording shows what the tab does for a
  renamed session. The generated title is kept first because that *is* measured. TC-112b
  states the gap rather than papering over it.

**The prompt is read by a function of its own, not by widening the allow-list.** Adding
`text` to `RawBlock` would build the text of every block of every message, assistant output
included. Instead `record::human_prompt_line` parses a second, narrower shape, and:

- reads only a `user` record whose `origin.kind` is `human` — a tool result is a `user`
  record too, and its content is neither a prompt nor a name;
- takes the **first line** of the first text block, trimmed — where a line break is `\n` or a
  bare `\r`, because `str::lines` does not treat a lone carriage return as one and a prompt
  written on a machine that uses them would have come back whole;
- caps it at 120 **characters**, never bytes;
- and answers in three ways, not two. A human prompt that yields nothing usable — an image, a
  line of spaces — reports `NoLine` rather than `NotAPrompt`, because it has been *seen*.
  Folding those two together let the search run on to the second prompt and the third until
  one had text, which would have made the stored line "the first prompt that parsed" rather
  than the opening one;
- and is called by `watch::live` **only until that session's first human prompt has been
  seen**, whether or not it yielded anything.

So what the product builds from what the user wrote is one short line per session, once. Not
a paragraph, not a later prompt, not anything an assistant said. The last two points were
found in review, and both were the difference between a promise and a habit.

## Consequences

- The board and the tab agree. That was the whole point.
- **The board now displays a line the user typed**, on an always-on-top window. The exposure
  is not new — the editor's own tab shows the same string, in the same room, to the same
  screen share — but it is worth stating plainly rather than discovering. FR-35's deferred
  "hide titles" mode is the thing that would cover both.
- `record.rs`'s promise is narrower and still exact: prompt text is not materialised *by the
  parser*, and the one function that does read it says what it reads and why. That is the
  reviewed decision its comment asked for.
- A session with no title yet stops showing a slug within a turn or two of starting, which is
  the window in which the board is least useful and the name mattered most.
- Nothing is persisted. The opening line lives in the watcher's map for the session's
  lifetime, is pruned with the session, and never reaches the settings file (FR-28, NFR-07).
