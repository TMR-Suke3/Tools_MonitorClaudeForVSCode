# Observation sources

How the product learns what Claude Code sessions are doing, and the limits of that
knowledge. See [ADR-0001](adr/0001-observe-local-state-files.md) for why this approach was
chosen, and [session-state-model.md](session-state-model.md) for what is derived from it.

> ⚠️ **Everything described here is an undocumented internal of Claude Code.** There is no
> published schema and no stability guarantee. Treat every field as optional and every
> record type as unknown until proven otherwise (see NFR-05 in
> [requirements.md](requirements.md)).

---

## 1. Ground rules

1. **Read-only.** The product opens these files for reading, never writes, never locks
   exclusively, never deletes. It also reads the operating system's **process table** (§2.4)
   — likewise read-only: process ids and image names, never command lines, and nothing is
   ever signalled or injected. The one exception to read-only is the opt-in hook
   installation of §5, which the user must explicitly enable.
2. **Never touch the private channels.** Some records carry an auth token or the path of an
   internal IPC pipe. The product **MUST NOT** read, store, log, or connect to any of them.
   It is a file and process observer, not a client. Known so far: `authToken` in `ide/*.lock`,
   `messagingSocketPath` in the session registry, the `sessions/*.key` files, and the
   opaque token in the transcript's `atis-latch` records. Assume the list is incomplete —
   a field that looks like a credential is treated as one.
3. **Never persist content.** Prompts, responses, and file contents pass through memory
   only; nothing but ids, paths, titles, statuses, and timestamps is kept.
4. **Trust the record, not the path.** Where a value is available both from a filename and
   from inside a record, the record wins (see §2.3).

## 2. What is read

All paths are relative to Claude Code's state directory, referred to here as `~/.claude/`.

### 2.1 Live session registry — `~/.claude/sessions/<pid>.json`

One small JSON file per running Claude Code process. This is the primary source of truth
for *which sessions exist right now*.

| Field | Used for |
|---|---|
| `sessionId` | The stable identity of the session; joins to the transcript file. |
| `pid` | Liveness checking (§3) and, indirectly, crash detection. |
| `procStart` | Process creation stamp — guards against PID reuse (§3). |
| `cwd` | The workspace. Grouping key of the board (FR-04). |
| `entrypoint` | Filter: only VS Code extension sessions are shown (FR-02). **Inherited from the environment, not derived from how the process was launched** — a `claude -p` run started from inside a VS Code session's shell also reports `claude-vscode`, so the filter is a strong hint, not proof. |
| `kind` | Filter: interactive sessions only; non-interactive runs are not the user's business here. |
| `startedAt` | Session age. |
| `version` | The Claude Code version that produced these files; recorded for compatibility triage (NFR-05). |
| `name`, `nameSource` | Fallback title when no AI title exists yet (§4). |

Fields carrying an IPC endpoint or credential are ignored by rule 2 above. Sibling files
that are not `<pid>.json` are not read.

### 2.2 Editor window registry — `~/.claude/ide/<port>.lock`

One JSON file **per VS Code window** that has the extension connected. Fields used:
`workspaceFolders` (the folders that window has open) and `pid`.

Used **only** to answer "which editor window should I raise when the user clicks this
session?" (FR-33). It is not used to enumerate sessions, and its auth token is never read.

> **The `pid` field does not name the window.** It names the VS Code *instance* — the main
> process — and is therefore identical for every window of that instance. Measured on 82
> accumulated lock files: 38 distinct pids, of which **31 carried more than one lock**, up
> to five, and every one of those named a different workspace. One process hosting several
> windows is the normal case here, not an edge case.

The window is identified by the **port instead**. The lock's file name is the port an
extension host is listening on, and the extension host is per window — so the process that
owns that listening port *is* the window, and it is also the parent of the `claude.exe` the
session runs in. That gives an exact session → window mapping with no path comparison
anywhere in it ([ADR-0009](adr/0009-map-a-session-to-its-window.md)).

> **The extension host listens on more than one port, and only one of them is registered
> here.** Measured 2026-08-28: one host was listening on 54851, 58915 and 59126, with a lock
> file under 59126 alone; a second on 16147 and 63698, with the lock under 16147. The order
> the process table returns them in means nothing, so **the port cannot be read off the
> process** — every port the host holds has to be offered to the lock files, and the one they
> claim is the answer ([ADR-0027](adr/0027-pick-the-editors-port-by-its-lock-file.md)). Taking
> the first made every click on that window's sessions a silent no-op.

> **`workspaceFolders` can legitimately be empty.** An editor window with no folder open says
> so, and there is then no folder basename in its title to match on — nothing can be raised
> for it, and the board reports that rather than raising something else (limitation L-6).
> Measured: 3 of 84 accumulated locks.

> **These files go stale.** Entries whose process exited long ago remain on disk. Every
> entry is validated against a live process before use (§3), and a workspace group is never
> created from a lock file alone.

> **The session's name is not in the registry — not the one you can see, anyway.** `name` is
> a slug built from the folder and the id (`notes-e8`), and `nameSource` says so: measured
> 2026-08-28, all four live sessions carried `"derived"`. The name on the editor's tab comes
> from the transcript instead — the `ai-title` record if there is one, the opening human
> prompt if there is not
> ([ADR-0030](adr/0030-a-session-is-named-what-its-tab-is-named.md)).

### 2.3 Transcript — `~/.claude/projects/<encoded-workspace>/<sessionId>.jsonl`

An append-only JSON-Lines log of the conversation, written as it happens. This is the
source of *activity* and therefore of status.

Record types observed so far, and what the product takes from each:

| `type` | Meaning for the board |
|---|---|
| `user` | A turn was submitted, or a tool result came back. Distinguishes a real prompt from a tool result by its content and its prompt metadata. A tool result can carry an error flag. |
| `assistant` | Claude produced output. Carries a **stop reason**, which is how the end of a turn is known — structurally, not by timeout. Values seen: `tool_use`, `end_turn`, `stop_sequence`, and **`null`**. A null stop reason means a *mid-stream* partial record, **not** a turn that ended without a reason — one logical turn is written as several records and only the last carries a reason. A trailing tool-use block with no matching result is the key signal for *waiting for the user* (see the state model). |
| `attachment` | Context attached to a turn. Ignored except as evidence of activity. |
| `ai-title` | **The session title** shown on the board (§4). The most recent one wins. Written during the turn, not only at its end. |
| `last-prompt` | Snapshot of the latest prompt text. Used only as a last-resort title fallback; never stored. |
| `queue-operation` | A prompt was queued / dequeued — an early "work is starting" signal. Only `operation` is read: the record also carries the **full prompt text** in a `content` field, which is dropped unread (NFR-07). |
| `system` | Out-of-band notices, distinguished by a subtype. Two matter: an **API error** subtype carrying the error and its retry metadata, and a **compaction boundary** subtype, which counts as activity, not as a pause. Note that a usage limit is **not** one of these — see the row below. |
| `mode` | The session's permission mode changed. Tracked because a mode that never prompts cannot be waiting on a permission prompt. |
| *(any record carrying `quotaLimits`)* | **The usage limit** (FR-15). Not a record type of its own and **not** an API error: it rides on an ordinary `assistant` record. `status: "rejected"` is the whole signal, `resetsAt` is a unix timestamp of when the session may continue, and `rateLimitType` names the window (`five_hour` in every observed case). The text block alongside it is the human-readable message and is **not read** — the status is decidable from the structure. Detail in §2.3.1. |
| `file-history-snapshot`, `file-history-delta` | File-edit bookkeeping. Ignored. |
| *(anything else)* | Skipped silently (FR-38). |

Message records also carry `cwd`, `gitBranch`, `sessionId`, `timestamp`, `uuid`,
`parentUuid`, `isSidechain` and `version`, which are used for joining and ordering.

> **Subagents are not in this file.** A session that dispatches a subagent does **not**
> write side-chain records into its own transcript. The subagent gets a **separate file**,
> `agent-<hex>.jsonl`, in the same directory, carrying its own `sessionId` and consisting
> entirely of `isSidechain: true` records. Measured across 78 transcripts: 840 side-chain
> records, every one of them in an `agent-*.jsonl`, and none in an ordinary transcript.
> Those files have no entry in the session registry, so discovery never picks them up
> (FR-57). What the parent shows instead is an ordinary pending tool call — see
> [session-state-model.md](session-state-model.md) §4.2.

> Record types observed while writing this document are not the complete set — several
> appeared only a handful of times across 62 sessions. Treat the list as "what is known",
> never as "what exists" (FR-38).

> **The directory name is lossy.** The workspace path is encoded into the directory name by
> replacing characters that are not filename-safe, so `my_repo` and `my-repo` can collapse
> onto the same directory. **Never decode it.** The real path is read from the `cwd` field
> of the records (and of the session registry).

#### 2.3.1 The usage limit, and why headroom is not shown

Measured on 2026-08-25 across 84 transcripts: **6 occurrences in 4 files**, every one with
an identical shape.

```json
{"status": "rejected", "resetsAt": 1787582400, "rateLimitType": "five_hour",
 "unifiedRateLimitFallbackAvailable": false, "overageStatus": "rejected",
 "overageDisabledReason": "out_of_credits", "upgradePaths": ["upgrade_plan"],
 "isUsingOverage": false}
```

What the measurement establishes:

- It arrives as an `assistant` record 0.4–0.7 s after the user's message or tool result —
  **the turn dies instantly** — and **no `api_error` accompanies it**. Nothing else in the
  file marks the event.
- `message.stop_reason` is `stop_sequence` on all six, a value that appears for no other
  completed turn in the corpus. A useful cross-check, but `quotaLimits.status` is the
  signal the product depends on; a stop reason is far likelier to be reused for something
  else in a future version.
- `resetsAt` is exact and worth showing: measured gaps from the rejection were 5.4, 5.7,
  41.5, 180.1, 180.9 and 215.3 minutes — a rolling window, never a fixed offset, so it
  cannot be computed and has to be read.
- **Recovery has no marker.** The limit clears when the next assistant record simply
  succeeds, which is what FR-15 keys on.
- One of the six is the last record in its file: the session was abandoned at the limit.
  A limited session is therefore also a candidate for `terminated`, and the order of those
  two rules matters (state model §7.2).

**Nothing warns before the wall.** The board cannot say "you are running low", only "you
are stopped". Checked three ways across the corpus: no record type and no `system` subtype
carries a warning, the only quota records are the six `rejected` ones, and a full-text
search for the wording returns matches exclusively from prompts, tool inputs, and file
snippets. If the editor displays a warning, it is UI-only — the same shape as the permission
prompt in §5. (Whether a hook fires for one is unmeasured; four of the five hook events have
never been observed.)

**Careful with `total_tokens_reminder`.** 1,208 attachment records carry a line reading
`<total_tokens>… tokens left</total_tokens>`, which looks exactly like the headroom figure
this section says does not exist. It is a **per-session budget**: every session that carries
it starts at the same 15,000,000, and at the moment of a real five-hour rejection it still
read 99.98 %. It is uncorrelated with the account limit and must never be shown as one.

**No remaining quota is published, anywhere.** `quotaLimits` has no field naming an amount,
a total, or a percentage; it says *you are cut off, and when you resume*. Consumption, by
contrast, is fully available — 8,786 records carry per-turn `input`, `output`, and cache
token counts. The product deliberately uses neither to estimate headroom: the numerator
without the denominator is not a percentage, and the denominator lives on the server, where
NFR-08 forbids the product from going. The other local candidates were checked and are
empty or stale (`stats-cache.json` had not been written for three months; `telemetry/` was
empty).

### 2.4 The process table

Not a file, and the one place this product looks beyond Claude Code's own state directory.

A tool that executes as a **subprocess** — a shell command, most obviously — appears as a
**child process of the Claude Code process** for exactly as long as it runs. Measured by
sampling a live session every 250 ms across an 18-second `Bash` tool: no children before, a
single `bash.exe` for the duration, none after, with the boundaries matching the tool's own
start and end to within one sample.

Because a `tool_use` record is written *before* the tool runs, the two sources combine into
a discrimination neither can make alone:

| Transcript | Process table | Meaning |
|---|---|---|
| tool call pending | a child is running for it | the tool is genuinely executing → `working` |
| tool call pending | nothing running for it | the tool never started → something is blocking it → `awaiting_user` ([ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md)) |

What is read is the process id, parent process id, creation time, and image name — the same
information already needed for liveness (§3). **Command lines are not read**: they contain
the user's shell commands, which is conversation content by another name (NFR-07). Nothing
is written, signalled, or injected; this is `ps`, not a debugger.

**Both directions are measured.** A permission prompt was held open on purpose, in manual
permission mode, across a `ping -n 15` call:

| Phase | Duration | Children |
|---|---|---|
| prompt on screen | **44.3 s** | **none** |
| approved, executing | 14.3 s | `bash.exe` |

The transcript recorded 58.2 s pending; the process table accounts for it as 44.3 s idle
plus 14.3 s running. The empty window is 19× the startup lag below, so it cannot be confused
with a tool that is slow to start, and the process appears at the moment of approval rather
than before it — Claude Code does not pre-spawn the shell and gate only the exec.

**A denial behaves the same way, and resolves itself.** Measured separately: a prompt that
the user refuses never produces a child process either, and Claude Code writes a
`tool_result` carrying `is_error: true` and a rejection message. So the pending call *ends* —
there is no state in which the transcript says "pending" forever while the user has already
answered. Nothing has to match the wording; the call ceasing to be pending is the whole
signal, and the existing rule in [session-state-model.md](session-state-model.md) §7.1 — an
errored tool result with the turn continuing is `working` — already covers what follows.

Three limits remain:
- **There is a startup lag before the child appears — measured at 2.3 s.** Between the
  `tool_use` record and the process becoming visible, the table is legitimately empty. Any
  rule reading absence as evidence must outwait it (`T_pending_probe` in
  [session-state-model.md](session-state-model.md) §3).
- **Not every tool spawns a process.** An in-process edit or file read never will, so their
  absence of a child means nothing. Those are covered by the outlier rule in
  [session-state-model.md](session-state-model.md) §4.1 instead. The probe is therefore
  framed as *"did any child appear during this pending window"*, never as a list of tool
  names — the list is exactly the kind of internal detail C-3 says will change.
**Sampling interval and cost.** The probe samples every **500 ms**, and may be relaxed to
**1000 ms** to trim cost. The interval does not have to track how long tools run, because
the probe is only ever consulted for a call that has already been pending for
`T_pending_probe` (10 s) — anything faster than that has its result and is no longer
pending. A subprocess tool pending that long holds its process for the whole window, so ten
samples across it is ample.

**One enumeration serves every watched session**: the product reads the process table once
and filters by parent pid, so ten sessions cost exactly what one does. NFR-03's ten-session
figure does not multiply this.

Measured on a 24-core Windows machine with ~350 processes running:

| Method | Per enumeration | @1000 ms, one core | @1000 ms, whole machine |
|---|---|---|---|
| `Win32_Process` via CIM | 249 ms | 24.9 % | 1.04 % |
| `CreateToolhelp32Snapshot` | 12.6 ms | 1.26 % | 0.05 % |
| **`NtQuerySystemInformation`** | **7.4 ms** | **0.74 %** | **0.03 %** |

The CIM query the spike used is **unusable** — a quarter of a core at 1 Hz. The native call
fits NFR-03 with room to spare, and it was measured through .NET marshalling that a native
implementation does not pay, so 7.4 ms is a ceiling rather than a floor.

Two caveats. These are one machine's numbers, and a busier or weaker one will differ. And
polling is not the only option: Windows can *notify* on process start and stop, which would
drop the idle cost to near zero — worth evaluating before settling on a timer.

### 2.5 Deliberately not read

- `~/.claude/history.jsonl` — a global prompt history; unnecessary for status and full of
  content.
- `~/.claude/backups/`, `file-history/`, `shell-snapshots/`, `debug/`, `downloads/`,
  `cache/` — content, not status.
- `~/.claude/settings.json` — read only when the user asks about the optional hooks (§5),
  and written only on explicit opt-in.
- Any credential, key file, or IPC socket path. Named explicitly because it came up as a
  means to an end: `~/.claude/.credentials.json` holds the OAuth token that would make a
  quota lookup possible. The product does not read it, and the feature that wanted it was
  dropped instead (§2.3.1).

## 3. Liveness and staleness

A record on disk is not evidence that anything is running.

- A session is **live** when its `pid` exists *and* the process creation stamp matches
  `procStart`. The second half matters: PIDs are reused, and a stale registry file whose
  PID now belongs to an unrelated process would otherwise resurrect a dead session.
- A session whose process stops matching has **ended**. Whether that ending was normal or
  abnormal is decided by the state model, not here.
- **A registry file disappearing is weaker evidence than it looks.** The directory is
  garbage-collected: when any Claude Code process exits cleanly it removes not only its own
  entry but every entry whose process is dead. Measured twice — 17 entries collapsed to the
  3 live ones in one sweep. So entries can vanish in bulk, long after the sessions they
  described ended, triggered by an unrelated process. Liveness is decided by the process
  check above, never by the file's presence alone.
- **A killed process leaves its entry behind; a clean exit removes it.** This is the only
  local signal that separates the two, and it is what §7.2 of the state model rests on.
- The `sessions/*.key` siblings are not swept and accumulate indefinitely. They are not
  read (§2.1).
- Lock files under `ide/` are validated the same way before being used for window focus.
- Anything that fails validation is ignored, not displayed, and not reported as an error
  (FR-40).

## 4. Session title

Resolution order, first hit wins:

1. The most recent `ai-title` record in the transcript — a short summary generated by
   Claude Code **in the language the conversation is held in**.
2. The `name` field of the session registry (a derived slug, e.g. the folder name plus a
   suffix).
3. The first line of the latest prompt, truncated.
4. The workspace folder name.

Consequences for the UI:

- Titles arrive in **any language**; Japanese titles are common. They are shown as-is —
  width, not character count, is what must be budgeted, and CJK must render (FR-19, FR-20).
- A title can **change mid-session** as the conversation moves on. The board updates in
  place; it must not treat a retitle as a new session.
- Early in a session there may be no AI title at all, so the fallback chain must always
  produce something.

### 4.1 The opening prompt is not always the first text block

A human `user` record's `message.content` is a **list of blocks**, and the editor puts what
it wants Claude to know in a text block of its own, *before* the one the user wrote:

```text
content: [ {"type":"text","text":"<ide_opened_file>…</ide_opened_file>"},
           {"type":"text","text":"…the prompt the user typed…"} ]
```

**Measured: 41 of the 69 recorded transcripts open this way** — `<ide_opened_file>` in 38,
`<ide_selection>` in 3. Taking the first text block therefore named the majority of sessions
after the editor's note, and the board read `<ide opened file>The user ope…` where the tab
read the question. The same case with an `image` first was already handled, because its
`type` is not `text`.

So a block is skipped when removing every complete `<tag>…</tag>` from it leaves nothing
behind — the shape rather than the tag names, because these are Claude Code internals with
no stability promise and a third tag would break a list. Removing the tags from the note
leaves nothing in **all 41**, which is what says the note is a whole block rather than a
prefix on the prompt.

A record whose every text block is a note has still been *seen*: the session keeps its
previous name, and the search does not run on to the second prompt (ADR-0030).

## 5. Optional accuracy layer: hooks

Claude Code can run user-configured hook commands on lifecycle events (session start/end,
prompt submitted, before/after a tool call, notification, stop). A hook that writes a small
JSON line to a local file gives the observer an *authoritative, immediate* signal instead of
an inferred one — most importantly for "a permission prompt is on screen" and "the turn is
over".

This is deliberately **not** required:

- It means editing the user's Claude Code settings, which the product should not do
  silently.
- It can collide with hooks the user already has.
- A user who never enables it must still get a working board.

So hooks are an **opt-in accuracy upgrade** (FR-37, [ADR-0001](adr/0001-observe-local-state-files.md)).
When enabled, hook events take precedence over inference; when not, the file-derived
inference stands on its own.

Because the setup is the part users are least willing to do by hand, **the product performs
it for them** — offer, preview, backup, write, verify, and one-click undo — rather than
documenting the JSON and hoping. That flow is specified in
[hook-setup.md](hook-setup.md) and decided in [ADR-0003](adr/0003-guided-hook-setup.md).

Five events are used: session start, prompt submitted, notification, turn stop, session
end. Events that run *before* a tool call are deliberately not used, because they are able
to block or alter the call they precede (FR-47). The exact event names and payloads are
pinned at implementation time against the Claude Code documentation of the day, and the
feature refuses to install itself if they do not match.

**One payload has been observed directly.** A `Stop` hook was installed once, on the
author's machine, and removed again. It delivered 963 bytes of JSON on stdin with these
keys:

| Key | Value for the board |
|---|---|
| `hook_event_name` | `"Stop"` — the event kind. |
| `session_id` | Joins to the session registry and the transcript. |
| `transcript_path` | **The absolute path of the transcript.** This is worth more than it looks: it removes any need to reconstruct the path from the lossy encoded directory name (§2.3). |
| `cwd` | The workspace. |
| `permission_mode` | **The live permission mode**, which is what §4.1 rule 1 needs and which inference can only read from an untimestamped `mode` record. |
| `stop_hook_active` | Guards against a Stop hook re-triggering itself. |
| `last_assistant_message` | ⚠️ **The full text of the assistant's last message.** |
| `effort`, `background_tasks`, `session_crons` | Not used. |

Three consequences:

- **The payload carries conversation content.** `last_assistant_message` is the whole
  message, and `user_prompt` on `UserPromptSubmit` is the whole prompt. FR-49 — the helper
  keeps the event kind, the session id, a timestamp and the notification kind, and nothing
  else — is therefore load-bearing, not a precaution. It is a stronger obligation than the
  transcript reader's: a transcript is a file this product chose to open, while a hook
  payload is handed to it.
- **There is no timestamp in the payload.** The helper has to stamp the event itself on
  arrival.
- **The vocabulary does not match the transcript's.** The hook reports
  `permission_mode: "default"`; the transcript's `mode` records say `"normal"` for what
  appears to be the same state. Neither vocabulary can be assumed to be the other's, and
  the two must be mapped explicitly rather than compared as strings.

A second source therefore exists only in hook mode: **the product's own event file**,
written by its hook helper into its own data directory. It contains the event kind, the
session id, a timestamp, and the notification kind — never conversation content (FR-49).

## 6. Reading strategy

- **Order by append position, never by `timestamp`.** The two disagree constantly: across
  78 transcripts, 56 contained records written out of timestamp order, 940 in total.
  `api_error` records are the worst case — they are appended *after* the turn they belong
  to has already ended, carrying their original, earlier timestamp. Several record types
  (`ai-title`, `last-prompt`, `mode`, `file-history-snapshot`, `atis-latch`) carry **no
  timestamp at all**, so byte offset is the only total order that exists — and it is the
  only one a tailer can observe.
- **Treat a record's own timestamp as content, not as arrival time.** Because of the above,
  "did this just happen?" must be answered by comparing the record's timestamp against the
  newest one already seen, not against the wall clock. A record describing a failure that
  was already retried and recovered must not raise an alarm minutes later.
- **Watch, do not poll** the small directories (`sessions/`, and the transcript directory of
  each live session). Fall back to a ~1 s poll where filesystem notifications are
  unreliable.
- **Tail by byte offset.** Transcripts reach hundreds of kilobytes within a single session
  and megabytes over a long one; only the bytes appended since the last read are parsed
  (NFR-02).
- **Handle partial lines.** The writer may be mid-append; an incomplete trailing line is
  buffered until its newline arrives.
- **Handle truncation and replacement.** If the file shrinks below the stored offset, or its
  identity changes, the offset resets and the file is re-scanned from the start.
- **On first attach, read back — do not start at the end of the file.** A board started
  while a session is *already* limited has only one piece of evidence, and it is a record
  written before the board existed. The same applies to a pending tool call and to the stop
  reason of the last turn: a tailer that begins at EOF knows nothing about any session that
  did not happen to act while it was watching. The back-scan is bounded (the last few
  hundred records of the live session's own transcript, far short of the megabytes NFR-02
  is about) and it stops at the first record that settles the current status.
- **A reset time is only current until activity resumes.** Transcripts are append-only, so
  every past rejection stays in the file with its old `resetsAt`. Only the newest one, and
  only while no successful turn follows it, describes the present.
- **Bound the work.** Only transcripts of live sessions are followed. Historical project
  directories — of which there can be dozens — are never scanned at startup beyond locating
  the live sessions' own files.
- **Decode as UTF-8** with replacement on invalid bytes; a malformed record must never take
  down the observer.

## 7. Known limitations

| # | Limitation | Consequence |
|---|---|---|
| L-1 | Two VS Code windows open on the same folder cannot be told apart *once the board has to name a window handle*. The session data itself now does distinguish them (§2.2), but the only route from a window to a raisable handle is its title, and both windows carry the same folder name in it. | They are shown as one workspace group (FR-07), and click-to-raise may bring up either window — stated in FR-33 itself. Measured frequency: the case never occurred once in 82 lock files, and `code --new-window` on an already-open folder focuses the existing window rather than making a second one, so it is reachable only through the in-editor *Duplicate Workspace in New Window* command. |
| L-2 | The formats are internal and may change without notice. | The product records the observed version, degrades to the neutral status, and warns (FR-39). |
| L-3 | Inference-only mode cannot see the editor's UI. It infers "waiting for the user" from a pending tool call combined with **evidence that the tool is not running** (§2.4) or that the call is a gross outlier for its own tool. | Good where either signal applies; silent where neither does — a tool that runs in-process *and* is behaving within its normal range is indistinguishable from a prompt. The first design, a flat 45-second timer, was measured at 14 % precision and replaced ([ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md)). Uncovered cases resolve to `working`, never to a false amber. |
| L-4 | ~~Telling a usage limit from any other API error depends on wording.~~ **Withdrawn — the premise was wrong.** A usage limit is not an API error and needs no wording match: it is a `quotaLimits` object with `status: "rejected"` on an `assistant` record (§2.3.1). The earlier statement came from looking at `error.rateLimits`, which is null on every observed error because limits never travel that way. | Detection is structural, and the reset time is exact. What remains unknown is narrower: only the `five_hour` window has ever been observed, so a differently-shaped limit (a weekly one, say) would be seen as *some* rejection with a reset time, which is still the right status. |
| L-5 | A turn that dies on an error is visible — errors are recorded structurally. A *process* that disappears leaves no record either way, but it does leave a **trace in the registry**: measured directly, a killed process leaves its `sessions/<pid>.json` behind, while a clean exit deletes it. | The error case is detected directly. For the process case the registry entry's fate is the primary evidence, with the last transcript state and the editor's liveness as the tie-breaker (§3, and the state model §7.2). The signal is not airtight: a *later* clean exit by an unrelated session sweeps the leftover entry too, so it must be read at the moment the process dies, not reconstructed afterwards. |
| L-6 | Two windows whose workspace folders share a **last path segment** (`.../a/docs` and `.../b/docs`) are indistinguishable by window title, because only the basename appears in it. Unlike L-1 these are genuinely different sessions. | Click-to-raise may reach the wrong window. The board itself is unaffected: grouping and status use the full `cwd`, so only the jump is approximate. A user-customised `window.title` breaks the match entirely, in which case the raise fails visibly rather than silently (ui-overlay.md §6.1). |
| L-7 | An editor window with **no folder open** carries no folder in its lock (`workspaceFolders: []`) and none in its title either — one measured live read carried the file on screen and nothing else — `"<the open file> - Visual Studio Code - Insiders"`. There is nothing to match on, and nothing else joins a window to its own extension host: the hosts run as `--type=utility` with no window id, two windows shared one `--vscode-window-config` guid, and both windows reported the *main* process as their owner. | Named by elimination instead ([ADR-0028](adr/0028-name-the-last-window-by-elimination.md)): an extension host is per window, so the instance's other hosts account for all but one of its windows, and if exactly one is left it is this one. **Two folderless windows on one instance cannot be told apart, and neither is raised** — the same answer L-1 gives, for the case where the folder is absent rather than duplicated. Measured: 3 of 84 accumulated locks name no folder. |
