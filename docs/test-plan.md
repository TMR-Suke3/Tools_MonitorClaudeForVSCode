# Test plan

> Status: **draft**. Nothing is implemented yet; this document says how the core will be
> verified when it is, and pins the expectations before any code exists.

§1 – §6 cover the **core** — session discovery, transcript tailing, the status state
machine, and title fitting: the parts that can be tested headlessly from recorded data
(NFR-10, NFR-11). §7 covers the **UI**, and was added after the visual design pass, which is
what the first revision of this document said it was waiting for.

Related: [requirements.md](requirements.md) (§11 defines what v1 must satisfy) ·
[session-state-model.md](session-state-model.md) · [observation-sources.md](observation-sources.md)

---

## 1. The three rules this plan runs on

1. **Fixtures are cut from real recordings, never written by hand.** A hand-written fixture
   encodes the same assumption the implementation would, and the resulting test passes
   without proving anything. Every `.jsonl` in `tests/fixtures/` was sliced out of an actual
   Claude Code transcript and anonymised (§3).
2. **The expectations below are fixed now and are not negotiable later.** When the
   implementation phase starts and a test fails, the thing that gets fixed is **the code** —
   not the fixture and not the expected status sequence. Changing an expectation requires
   changing the requirement it comes from, in `requirements.md`, in its own commit.
3. **One test case = one fixture + one expected status sequence + one requirement id.** A
   test that cannot name the requirement it defends does not get written, so coverage can be
   audited by requirement rather than by line count.

A fourth rule governs honesty: **"could not be measured" is a result.** Rows marked
*unverified* below are not gaps to be quietly filled with a plausible guess — they are
recorded as unknown, and the behaviour they describe stays best-effort until someone
measures it.

## 2. What the fixtures are made of

The corpus was the author's own machine on 2026-08-25: **78 transcripts, 19,392 records,
89 MB**, spanning Claude Code 2.1.138 – 2.1.241, plus 17 session-registry entries and 84
editor lock files. Raw measurements are in the working notes; the parts that changed this
plan are called out in §6.

One fixture — `usage-limit-rejected.jsonl` — was cut later, from the same machine with the
corpus grown to **84 transcripts**. It is noted here rather than folded into the figure
above, so the number keeps meaning "what was measured when this plan was written".

## 3. Anonymisation

Transcripts contain prompts, source code, file paths, and at least two credential-shaped
values. Nothing is committed until it has been through
`scrub()`, which works from an **allow-list**: any field not explicitly named is dropped.
(The first attempt used a deny-list and leaked absolute paths nested inside
`file-history-snapshot` records — an allow-list cannot fail that way.)

| | |
|---|---|
| **Preserved exactly** | record order, `type`, `subtype`, `stop_reason`, `isSidechain`, `entrypoint`, `version`, `retryInMs` / `retryAttempt` / `maxRetries`, `source`, `operation`, tool **names**, content-block **types**, `is_error`, `compactMetadata` numbers, `quotaLimits` (enums, flags — and `resetsAt`, which is shifted with the timestamps so its distance from the rejection survives) |
| **Shifted by one constant** | every `timestamp`. The recording's real wall-clock times said when the author was at the keyboard; no test needs that. A single global offset preserves every inter-record gap to the millisecond, the record order, and the out-of-order appends, while removing the absolute anchor. `procStart` / `startedAt` in the registry fixture are synthetic for the same reason |
| **Remapped** | `sessionId`, `uuid`, `parentUuid`, `logicalParentUuid`, tool-use ids — stable within a fixture, so joins still work |
| **Replaced** | `cwd` → `C:/work/sample-repo`, `gitBranch` → `feature/sample`, `aiTitle` → a harmless title of identical width |
| **Dropped** | every text body: message text and thinking, tool inputs, tool results, `queue-operation.content` (the full prompt), `last-prompt`, `atis-latch.atis`, file-history payloads, `usage`, `diagnostics` |
| **Never read at all** | `authToken` in `ide/*.lock`, `messagingSocketPath`, the `sessions/*.key` files |

The anonymised stream must remain **equivalent as state-machine input**: timing, ordering,
and every field the state machine reads are untouched.

**Verifying it.** The check decodes each record as JSON before matching, because the obvious
`grep` does not:

```
grep -rniE "C:\\\\Users|authToken" tests/fixtures/     # MISSES the leak
```

A path inside a JSON string is escaped as `C:\\Users`, so a regex looking for one backslash
never fires. The real check parses the JSON and walks the decoded values; it lives with the
spike scripts and must pass before any fixture is committed.

## 4. Fixture inventory

| Fixture | Cut from | What makes it worth keeping |
|---|---|---|
| `turn-normal.jsonl` | a routine turn | tool calls → `end_turn` → 12.7 h of silence → a new prompt |
| `api-error-retrying.jsonl` | a 401 that recovered | two `api_error` records, `retryAttempt` 1 and 2 of `maxRetries` 10 |
| `api-error-late-append.jsonl` | the same event | the errors are appended **after** the turn already ended, carrying older timestamps |
| `api-error-connection.jsonl` | an `ECONNRESET` | a different `error` shape: `error.connection` set, `error.status` absent |
| `compaction.jsonl` | a manual compaction | `durationMs` 155,638 — a 2 m 36 s silence, bridged by a queued prompt |
| `subagent-parent-pending.jsonl` | a parent session | an `Agent` tool-use pending **354 s**, with nothing else written |
| `subagent-child.jsonl` | an `agent-*.jsonl` | 100 % `isSidechain`, its own `sessionId` |
| `pending-interactive.jsonl` | an `AskUserQuestion` | pending **2,275 s**; the transcript is empty for the whole wait |
| `pending-ordinary-long.jsonl` | a `PowerShell` call | pending **4,737 s**; indistinguishable from the row above *in the transcript*. Telling them apart is what the process probe is for ([ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md)) |
| `title-change.jsonl` | a retitled session | two different `aiTitle` values in one session |
| `tail-untimestamped.jsonl` | a session that stopped | ends on `last-prompt`, which carries no `timestamp` |
| `unknown-record-types.jsonl` | a normal session | contains `atis-latch`, a type no specification mentions |
| `entrypoint-desktop.jsonl` | a Claude Desktop session | `entrypoint: claude-desktop` — must be filtered out |
| `streaming-null-stop-reason.jsonl` | a streamed turn | assistant records with `stop_reason: null` mid-turn |
| `titles.json` | 98 real titles | width-preserving substitutes, plus derived grapheme cases |
| `session-registry.json` | 17 real entries | the measured 3-live-of-17 ratio, two workspaces sharing a `cwd` |
| `prompt-observed.jsonl` | a **real permission prompt**, held open on purpose | The only fixture whose events are not all transcript records. `tool_use`, then **44.3 s with no process for the pending call**, then the process starting at the moment of approval, then 13.9 s of it running, then `tool_result`. This is the recording that proves the negative half of [ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md) |
| `usage-limit-rejected.jsonl` | a session cut off by the five-hour limit | a `quotaLimits` record with `status: "rejected"`, its exact `resetsAt`, and **no `api_error` anywhere in the slice** — the shape the first pass looked for in the wrong place (§6.3). The limit message text is dropped, so nothing may key on the wording |
| `prompt-denied.jsonl` | the same prompt, **refused** | `tool_use`, no process, then a `tool_result` carrying `is_error`. Proves the wait ends on refusal as well as on approval, so `awaiting_user` needs no timeout to escape. The rejection *wording* is dropped on purpose — nothing may depend on it |

## 5. Requirement → test case

Status names are those of [session-state-model.md](session-state-model.md) §1. "→" marks a
transition; a status in **bold** is the assertion the case exists for.

### 5.1 Discovery

| Req | Test case | Fixture | Expectation |
|---|---|---|---|
| FR-01 | TC-01 Enumerate sessions from the registry alone | `session-registry.json` | Every live entry is discovered with no user configuration |
| FR-02 | TC-02 Non-VS-Code entrypoints are excluded | `entrypoint-desktop.jsonl`, `session-registry.json` | A `claude-desktop` / `claude-cli` session produces **no board entry at all** |
| FR-03 | TC-03 A registry entry appearing/vanishing mid-run is picked up | `session-registry.json` | Add and remove entries between polls; the set changes without a restart |
| FR-04 | TC-04 Grouping key is `cwd`, label is its last segment | `session-registry.json` | Group label is `sample-repo`, never the full path, never the encoded directory name |
| FR-05 | TC-05 Several sessions per group, several groups | `session-registry.json` | Two entries sharing a `cwd` land in one group; other groups coexist |
| FR-06 | TC-06 A session that is not running occupies no space | `session-registry.json` | Stale entries produce nothing (the FR-59 exception is TC-23) |
| FR-07 | TC-07 Two editors on one folder are one group | `session-registry.json` | Two live entries with identical `cwd` → exactly one group |
| FR-40 | TC-08 Stale entries are ignored | `session-registry.json` | 14 of 17 entries have dead PIDs → not shown, and **not reported as an error**. Also: a live PID whose `procStart` disagrees is treated as dead (PID reuse) |

### 5.2 Status — the state machine

Every case here is a pure `(ordered events) → (status sequence)` assertion (NFR-11). The
event sequence is the **merge** of transcript records, registry appearances and
disappearances, process-liveness changes, tool-process start/stop, subagent-file activity,
hook events and timer ticks — not transcript records alone. Cases that need a non-transcript
event say so in the fixture column; those events are supplied by the harness, because the
core is forbidden from asking the OS for them.

| Req | Test case | Fixture | Expected status sequence |
|---|---|---|---|
| FR-08 | TC-09 Exactly one status at every point | `turn-normal.jsonl` | `working` … → **`idle`** at the `end_turn` record; never two statuses at once |
| FR-08 | TC-10 `stop_reason: null` does **not** end a turn | `streaming-null-stop-reason.jsonl` | Stays **`working`** across the null-stop-reason records; ends only at `end_turn` |
| FR-08 | TC-11 `stop_sequence` **does** end a turn | `api-error-retrying.jsonl` | Record 3 (`stop_sequence`) → **`idle`** |
| FR-09 | TC-12 No network, no model | all | The core is a pure function; the test harness has no network and no model client |
| FR-10 | TC-13 Unparseable input → neutral status | derived from `turn-normal.jsonl` | Corrupt one record → **`unknown`**, never *absent*, never a guess |
| FR-13 | TC-14 Always-interactive tool → waiting | `pending-interactive.jsonl` | `working` → **`awaiting_user` (high confidence)** 1.5 s after the `AskUserQuestion` tool-use → `working` when the result lands 2,275 s later |
| FR-13 | TC-15 An ordinary pending call never turns amber on time alone | `pending-ordinary-long.jsonl` | With no process evidence supplied, the 4,737 s pending `PowerShell` stays **`working` for its whole duration**. The retired `T_pending_ambiguous` behaviour must not reappear ([ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md)) |
| FR-13 | TC-15a A pending call with its process running is working | `prompt-observed.jsonl`, from `tool_process_started` onwards | Stays **`working`** for the 13.9 s the process runs. This is the case rule 3 used to get wrong 54 times out of 63 |
| FR-13 | TC-15b A pending call with no process is a wait | `prompt-observed.jsonl` (recorded across a real permission prompt) | **`awaiting_user` (high confidence)** after `T_pending_probe`, then `working` at the moment the process appears. The recording holds 44.3 s of an empty process table followed by 14.3 s of `bash.exe`, so both halves come from one measured sequence |
| FR-13 | TC-15e A refused prompt clears itself | `prompt-denied.jsonl` | `awaiting_user` → **`working`** the moment the errored `tool_result` lands. No timeout is involved, and the session must not stay amber |
| FR-13 | TC-15c Unknown process state is not evidence | `pending-ordinary-long.jsonl` + a *probe unavailable* event | Stays **`working`**. The platform failing to answer must resolve to the calmer status, never to amber |
| FR-13 | TC-15d An in-process tool that is a gross outlier is a wait | derived from `turn-normal.jsonl` | An `Edit` pending far beyond its own observed distribution → **`awaiting_user` (high confidence)**, with no process evidence involved |
| FR-13 | TC-16 A short pending call never flickers | `turn-normal.jsonl` | Sub-second tool calls stay **`working`** throughout; no transition is emitted |
| FR-11 | TC-17 `idle` asserts nothing about the work | `turn-normal.jsonl` | The status carries no completion verdict and no field that could render as one |
| FR-12 | TC-18 Time-in-status comes from the newest **timestamped** record | `tail-untimestamped.jsonl` | The file ends on `last-prompt` (no timestamp); elapsed time is measured from record 7, not from the last line |
| FR-14 | TC-19 Unrecovered error ends the turn → abnormal stop | **none — unverified** | See §6.2. No exhausted-retry sample exists |
| FR-58 | TC-20 A retry in flight is **not** an abnormal stop | `api-error-retrying.jsonl` | The two `api_error` records (retry 1/10, 2/10) leave the status **unchanged** — never `terminated` |
| FR-58 | TC-21 A stale error does not resurrect an ended turn | `api-error-late-append.jsonl` | The `api_error` records arrive after the turn ended and carry **older** timestamps than the newest record already seen → status stays **`working`** (from the queued prompt); no `terminated`, no `limited` |
| FR-58 | TC-22 A connection error has a different shape | `api-error-connection.jsonl` | `error.connection` set and `error.status` absent parses without error; status unchanged |
| FR-59 | TC-23 A killed session keeps its slot | `session-registry.json` | Process gone while `working`, **entry left behind** → **`terminated`**, visible for `T_terminated_visible`, then absent |
| FR-59 | TC-24 A clean shutdown is not a crash | `session-registry.json` | Process gone while `working`, **entry removed with it** → **absent**, never `terminated`. Same from `idle` regardless of the entry |
| FR-40 | TC-24b A bulk sweep is not a wave of deaths | `session-registry.json` | Delete 14 stale entries in one tick, as Claude Code's own garbage collection does → **no status change for anyone**, and no `terminated` for any of them |
| FR-15 | TC-25 A rejected quota enters the limit status | `usage-limit-rejected.jsonl` | A `quotaLimits` record with `status: "rejected"` → **`limited`**, taken from the structure alone. The fixture carries no message text, so a test that needs the wording cannot pass |
| FR-15 | TC-25b The limit clears with no marker for it | `usage-limit-rejected.jsonl` | Nothing announces recovery: the next successful assistant record ends the status. Replaying past it must leave `limited` behind without any dedicated event |
| FR-15 | TC-25c The reset time is read, never computed | `usage-limit-rejected.jsonl` | `resetsAt` is surfaced as given. The measured gaps run from 5.4 to 215.3 minutes, so a fixed offset from the rejection is wrong by up to three and a half hours |
| FR-15 | TC-25e Both ends of the wait are emitted | `usage-limit-rejected.jsonl` | `limited` carries the reset instant **and** the rejection instant that named it — 5.7 minutes apart in this fixture. A display cannot derive the length of the wait from the reset alone, and the core is the only layer that sees the transcript |
| FR-15 | TC-25d A limit is not an API error | `usage-limit-rejected.jsonl`, `api-error-retrying.jsonl` | The limit fixture contains **no** `system/api_error`, and the api_error fixtures contain no `quotaLimits`. A detector keyed on either one alone fails exactly one of the two |
| FR-56 | TC-26 Compaction is activity | `compaction.jsonl` | Stays **`working`** across the `compact_boundary`; specifically never `idle` between the queued prompt and the records that follow the boundary |
| FR-57 | TC-27 A parent with a live subagent stays working | `subagent-parent-pending.jsonl` + `subagent-child.jsonl` | **`working` for the entire 354 s** the `Agent` call is pending, because the subagent file is still growing ([ADR-0007](adr/0007-detect-subagents-from-their-own-file.md)) |
| FR-57 | TC-28 A subagent is never a session | `subagent-child.jsonl` | An `agent-*.jsonl` produces **no board entry**, whether or not it is discovered |
| FR-57 | TC-28b The exemption is not unconditional | `pending-ordinary-long.jsonl` | With **no** subagent file growing and no process evidence, the same 4,737 s pending call stays `working` — but a *tool-process never started* event still produces `awaiting_user` (TC-15b). The ADR-0007 exemption must not suppress ADR-0008's evidence |

### 5.3 Title fitting

| Req | Test case | Fixture | Expectation |
|---|---|---|---|
| FR-19 | TC-29 Titles pass through unmodified | `titles.json` | Output equals input whenever it fits: no summarising, translating, or word-stripping |
| FR-19 | TC-30 The fallback chain always yields something | `titles.json` (empty row), `session-registry.json` | Empty/absent `aiTitle` → registry `name` → prompt line → folder name |
| FR-20 | TC-31 Truncation is by rendered width | `titles.json` | A 13-char / 18-cell title and a 14-char / 28-cell title truncate at **different character counts** for the same budget |
| FR-20 | TC-32 Full-width text is never clipped mid-glyph | `titles.json` | For every measured row and every budget from 1 to its full width, the result's rendered width is ≤ budget |
| FR-21 | TC-33 Grapheme clusters survive | `titles.json` (derived rows) | ZWJ emoji, a combining acute, and a variation selector are never split; no lone surrogate is ever emitted |
| FR-21 | TC-34 A cut is marked | `titles.json` | Any truncated result ends in an ellipsis, and the ellipsis is inside the budget |
| FR-22 | TC-35 The full title stays reachable | `titles.json` | Truncation returns the fitted string **and** retains the original |
| FR-19 | TC-36 A retitle is not a new session | `title-change.jsonl` | Two `aiTitle` values in one session → the entry updates in place; the session identity and its position do not change |

### 5.4 Resilience

| Req | Test case | Fixture | Expectation |
|---|---|---|---|
| FR-38 | TC-37 Unknown record types are skipped | `unknown-record-types.jsonl` | `atis-latch` (and any type not in the known list) is skipped silently; the surrounding statuses are unaffected |
| FR-38 | TC-38 Malformed and partial records are tolerated | derived from `turn-normal.jsonl` | A truncated final line is buffered until its newline; a corrupt line is skipped; neither kills the observer |
| FR-39 | TC-39 Unrecognised data degrades visibly | derived | Force an unparseable transcript → **`unknown`** plus exactly one non-blocking warning, never a confident wrong status |
| NFR-02 | TC-40 Reading is incremental | `turn-normal.jsonl` | Append to a fixture and assert only the appended bytes were read; re-reading from offset 0 fails the test |
| NFR-02 | TC-41 Truncation and replacement reset the offset | derived | Shrink the file below the stored offset → re-scan from the start rather than reading garbage |
| NFR-05 | TC-42 The observed version is recorded | all `.jsonl` | The `version` field is captured per session; a failure is contained to that session |
| NFR-07 | TC-43 No content is ever persisted | all | Assert the retained model holds only ids, path, title, status, timestamps. **Specifically: `queue-operation.content` is a full prompt and `atis-latch.atis` is a credential — both must be dropped unread** |
| NFR-10 | TC-44 The core has no OS-specific code | — | The core crate builds and its tests pass with no Windows-only dependency |
| NFR-11 | TC-45 Derivation is deterministic | all `.jsonl` | Replaying a fixture twice yields identical status sequences; replaying it in one batch and record-by-record yields the same result |
| NFR-11 | TC-45a The core never reads the ambient system | all | Run the whole suite with the clock, the filesystem and the process table made unavailable to the core. Every status must still be produced, because time and liveness arrive **as events in the sequence**, not as calls. A core that reads `now()` fails this test |
| NFR-11 | TC-46 The ordering key is append position | `api-error-late-append.jsonl` | Events are ordered by byte offset, **not** by `timestamp`. Sorting by timestamp must change the outcome, and the timestamp-sorted order is the wrong one |

### 5.5 Latency

| Req | Test case | Expectation |
|---|---|---|
| NFR-01 | TC-47 Transition latency, core half | From "the event enters the sequence" to "status emitted" is under 100 ms for a fixture-sized append, leaving the rest of the 2 s budget to the watcher and the UI. The debounces (`T_pending_interactive` 1.5 s, `T_pending_probe` 2 s) are **specified behaviour, not latency** and are excluded from this measurement. The end-to-end figure belongs to a later phase |

### 5.6 Hook mode — the right-hand column of the state model §3

Added when the hook layer stopped being write-only. §7.3 still holds for the *setup flow* —
the settings edit, the consent steps, the refusals — which is a file-editing flow rather than
core logic. What is covered here is what happens to a **status** once events arrive.

Two honesty notes carry over from §6.2. **U-5 is only partly resolved**: of the five installed
events, one (`Stop`) has been observed as a real payload and four have not, so the *names* in
TC-93 are pinned against Claude Code's documentation (FR-53) rather than against a recording.
And the hook events in these cases are **supplied by the harness**, exactly as §5.2 requires
for every non-transcript event — the transcripts underneath them are real.

| Req | Test case | Fixture | Expectation |
|---|---|---|---|
| FR-36 | TC-82 The prompt-submitted event is activity | harness | An idle session → **`working`**, at high confidence, with no record needed |
| FR-13 | TC-83 The notification event is a wait, at once | `prompt-observed.jsonl` + harness | **`awaiting_user` (high)** within 200 ms of the `tool_use` — before `T_pending_interactive` and long before `T_pending_probe`, so neither debounce can be what produced it |
| FR-13 | TC-84 Only a permission prompt may summon | harness | Of the five signals, **exactly one** reaches `awaiting_user`. An idle notification is not a blocking UI and must not be turned amber (§9) |
| FR-08 | TC-85 The turn-stop event ends the turn | harness | → **`idle`** at high confidence |
| FR-59 | TC-86 A clean session end is not a crash | harness | The session leaves the board and is **never `terminated`** — this is the half §7.2 cannot read from a process disappearing |
| FR-59 | TC-87 A vanished session that never reported an ending | harness | Registry evidence absent + hooks live → **`terminated` (high)**, keeping its slot; the same input in inference mode is *absent*. Registry evidence, where it exists, still decides |
| FR-13 | TC-89 A live hook layer replaces the inference | `prompt-observed.jsonl` | The recording that proves rule 3 (TC-15b) must **not** produce `awaiting_user` when hooks are live and have reported no prompt: rules 2–3 are skipped, not consulted |
| FR-52 | TC-88 Silence falls back to inference | `prompt-observed.jsonl` + harness | Below `T_hook_quiet` the hook decides and the pending call stays **`working`**; past it, rule 3's evidence applies again and the same call becomes **`awaiting_user`** |
| FR-15 | TC-90 A limit outranks a reported wait | `usage-limit-rejected.jsonl` + harness | A reported prompt and a reported turn-stop both leave the session **`limited`** (§8), with the reset instant intact |
| NFR-11 | TC-91 The answer does not depend on the order | `prompt-observed.jsonl` + harness | The `tool_use` record and the notification, folded in **either order**, give the same status. This is the case that fails if a hook signal is ever written straight to the status ([ADR-0020](adr/0020-hook-signals-are-facts-not-status-writes.md)) |
| FR-13 | TC-92 A reported wait ends when the prompt is answered | `prompt-observed.jsonl`, `prompt-denied.jsonl` | Approving (the tool's process appears) and refusing (an errored `tool_result`) both end it, with no timeout involved — as TC-15e requires of the inferred one |
| FR-36 | TC-93 The five event names become five signals | harness | Claude Code's vocabulary stops at the board; what crosses into the core is a fact about the session. **Four of the five names are unverified** (U-5) |
| FR-13 | TC-94 A notification that is not a prompt is dropped | harness | `notification_type` other than `permission_prompt`, and a missing one, produce **no signal at all** |
| FR-36 | TC-95 Events are grouped by session | harness | The log is shared by every session on the machine; lines for sessions the board does not show are ignored rather than mis-attributed |
| FR-52 | TC-96 A line this build cannot use is still a sign of life | harness | An unknown event name still counts towards "events are arriving". A board that counted only what it understood would report the hooks as silent |
| FR-52 | TC-97 A replaced log does not replay itself | harness | Trimming resets the byte offset; events already folded in are dropped. An old `Notification` delivered twice is a summons nobody asked for |
| FR-52 | TC-97b A line sharing the boundary millisecond is not lost | harness | The dedupe cannot key on the timestamp alone: the helper stamps in whole milliseconds and several processes append, so two events can share one. The line already read is dropped; a different line at the same stamp is not |
| FR-36 | TC-97c A signal for an undiscovered session survives one poll | harness | A session registers a moment before the board discovers it. A signal nobody claimed is offered once more and then dropped — the log is tailed by byte offset, so dropping it immediately loses it for good, and keeping it for ever leaks the terminal sessions this board never shows |
| FR-13 | TC-98 A reported wait outlives the silence | `prompt-observed.jsonl` + harness | Nothing answers a reported prompt for longer than `T_hook_quiet`. The session stays **`awaiting_user`**: falling back to inference means the hook layer stops being consulted, not that what it last reported became false. Prompts do sit that long — 38 minutes measured for one, 4,737 s for a pending call |
| FR-13 | TC-99 Hooks arriving do not cancel an inferred wait | `prompt-observed.jsonl` + harness | Inference summons the user through rule 3; the hook layer then starts reporting, but says nothing about a prompt. The session stays **`awaiting_user`** — "no hook said so" also happens when a `notification_type` is unknown or a line was torn, and an absence must not overrule positive evidence |

**TC-19's counterpart here does not exist either.** There is no test that an *unrecovered
error* reported by a hook produces `terminated`, because §3 gives that case to the transcript
in both modes — the hook layer adds nothing to it.

## 6. Where measurement disagreed with the specification

None of these was resolved by quietly editing whichever side was cheaper to change. §6.1 was
a genuine contradiction and went through an ADR; §6.2 is a list of things nobody has
measured yet, kept visible rather than filled in with a plausible guess; §6.3 is a case
where this plan itself was wrong.

### 6.1 FR-57 contradicted state-model §4.1 rule 3 — **resolved**

Subagents do **not** write into the parent transcript. They get their own
`agent-<hex>.jsonl` with their own `sessionId`; across the whole corpus, sidechain records
appeared in ordinary transcripts **zero** times out of 840.

So from the parent's side a running subagent is just a pending tool call — and a long one:

| `Agent` tool calls observed | 15 |
|---|---|
| p50 pending | 1.8 s |
| p90 / max pending | 353.9 s / 367.1 s |
| **pending longer than 45 s** | **6 of 15** |

Rule 3 *as it then stood* turned any pending call into `awaiting_user` after
`T_pending_ambiguous` = 45 s, while FR-57 requires `working`. On this data the two
disagreed on **40 % of real subagent runs** — which is why the case was written down before
any code existed.

**Settled by [ADR-0007](adr/0007-detect-subagents-from-their-own-file.md):** while the
subagent's own file is being appended to, the parent's pending call is exempt from rule 3.
TC-27 asserts the exemption; TC-28b asserts it does not leak into ordinary pending calls.

Two committed documents also described subagents incorrectly and were corrected:
`observation-sources.md` §2.3 and `session-state-model.md` §4.2.

### 6.2 What could not be measured

Recorded as unknown rather than guessed. Each is a test case that exists but cannot be
written yet.

| # | Unknown | Consequence |
|---|---|---|
| U-1 | **How an exhausted retry appears.** No record with `retryAttempt == maxRetries` exists in the corpus, and nothing marks retries as given up. | TC-19 cannot be written. FR-14's error half stays best-effort. |
| ~~U-2~~ | ~~What a usage limit looks like.~~ **Measured — the field was wrong, not the event.** Limits do not travel in `error.rateLimits` at all; they arrive as a `quotaLimits` object on an `assistant` record. Six occurrences, four files, one shape (§6.3). | Resolved. TC-25 … TC-25d are writable and rest on a structural signal. L-4 in [observation-sources.md](observation-sources.md) is withdrawn: detection needs no wording match. |
| ~~U-3~~ | ~~Whether a killed process leaves its registry entry behind.~~ **Measured.** A killed process leaves `sessions/<pid>.json`; a clean exit removes it — and sweeps every other dead entry at the same time. | Resolved. TC-23 / TC-24 now rest on a measured signal, and TC-24b covers the sweep. The residual unknown is smaller: the distinction is only available to an observer that was watching at the moment of death. |
| U-4 | **Bypass-style permission modes.** All 78 `mode` records say `normal`. | §4.1 rule 1 has no sample and cannot be tested from this corpus. |
| ~~U-5~~ | ~~Real hook payloads.~~ **One measured.** A `Stop` hook was installed once and removed; its payload is documented in [observation-sources.md](observation-sources.md) §5. The other four events remain unobserved. | Partly resolved. Enough to know the envelope, the `transcript_path` shortcut, and that payloads carry conversation content (FR-49). The hook layer still has no test cases here — it needs its own plan (§7). |
| U-6 | **Automatic compaction.** The one observed boundary was `trigger: "manual"`. | An automatic trigger presumably exists but is not covered. |

### 6.3 U-2 was measurable all along — the plan named the wrong field

The first pass concluded that a usage limit had "never been observed" because
`system/api_error` → `error.rateLimits` was `null` on every error in the corpus. That field
is null because **limits never travel that way**. A limit is an ordinary `assistant` record
carrying a top-level `quotaLimits` object with `status: "rejected"`; there is no `api_error`
alongside it at all. Six occurrences across four transcripts, one identical key set, found
by grepping for the field nobody had thought to name.

The failure was not the measurement. It was that the specification had already decided
*where* to look, and the spike checked that place instead of checking for the event. **A
null field is evidence about the field, not about the event** — the same lesson §14 of the
working notes recorded about a different negative result. Where a spike concludes "never
observed", the entry now has to say what it searched for, so the next reader can tell an
absent event from an unexamined one.

What it costs to have got this wrong for one pass: nothing yet, because no code exists. That
is the entire argument for this phase.

## 7. The UI

Written after the visual design pass, which is what §7 of the previous revision said it was
waiting for. The design is [ui-overlay.md](ui-overlay.md); every criterion below quotes a
number from it, so the test fails against a specification rather than against a taste.

**One thing changed shape when the design landed.** Two requirements that looked inherently
visual — FR-16 (the palette) and FR-17 (a second, non-colour channel) — turn out to be
checkable from the palette constants alone, with no window, no screenshot and no human.
That is a direct consequence of deriving the palette instead of picking it
([ADR-0010](adr/0010-derive-the-palette-from-a-luminance-ladder.md)). They are therefore
**automated** cases in the same suite as the core, and they hold rule 2 of §1: when TC-48
fails, the hex changes, not the threshold.

The rest need a running board and are **manual**, executed against a build. Saying so is the
point: a checklist that pretends to be automated is worse than one that admits it is not.

### 7.1 Automated — the palette is data

| Req | Test case | Expectation |
|---|---|---|
| FR-16 | TC-48 Every indicator clears 3:1 against its own board background | Both themes, all six statuses including `unknown`. Computed from the hexes in `ui-overlay.md` §3.2. Tightest currently: **`idle` dark, 3.31:1** — it sits on the floor by design ([ADR-0013](adr/0013-make-idle-the-faintest-thing-on-the-board.md)), so this assertion is the one that catches a board background being darkened |
| FR-16 | TC-49 Tier-A pairs stay 15 dE2000 apart in every view | Normal vision plus protanopia, deuteranopia and tritanopia (Machado 2009, severity 1.0). Tier A is every pair involving `awaiting_user`, plus `terminated` against `working` and against `idle` |
| FR-16 | TC-50 Tier-B pairs stay 8 dE2000 apart in every view | The remaining pairs, both themes |
| FR-16 | TC-51 Greyscale separation | `awaiting_user` vs `terminated` ≥ 15 dE2000 in greyscale — the only pair sharing both a solid fill and a blink. Every other pair ≥ 4 |
| FR-16 | TC-52 The lightness ladder holds | Dark theme: `awaiting_user` ≥ `working` + 10 L\*, `working` ≥ `limited` + 8, `limited` ≥ `terminated` + 4, `terminated` ≥ `idle` + 3. It is a regression guard on the shape of the palette, and the constraint a future "nicer" colour is most likely to break silently. It is **not** a claim that prominence follows urgency throughout — see TC-52c for the part that is |
| FR-16 | TC-52c The two ends hold | **Dark theme:** `awaiting_user` has the highest ink index of all six statuses. **Both themes:** it is the largest mark the board draws, 160 px² — the halo, which is the channel that survives the light theme's inverted contrast, where `awaiting_user` sits third in ink and is not required to lead ([ADR-0018](adr/0018-the-loud-end-is-a-dark-theme-property.md)). The `idle` end is TC-52a. The middle of the ladder is deliberately not an urgency ranking, so nothing asserts one ([ADR-0014](adr/0014-the-ladder-pins-the-ends-not-the-middle.md)) |
| FR-16 | TC-52a `idle` is the least prominent of the five | Both themes — the one end of the ladder that survives the light theme unchanged. Ink index — indicator area × contrast against its own board — is lowest for `idle` of the five displayed statuses. Colour alone does not settle it, because the silhouettes differ in area |
| FR-16 | TC-52b Nothing renders below the contrast floor | No status, in any state the board can put it in, falls under 3:1. Specifically there is **no opacity ramp**: `idle` at `#676B6F` breaks the floor at 92 % opacity — composited in sRGB, the way the WebView will ([ADR-0017](adr/0017-judge-opacity-by-srgb-compositing.md); ADR-0013's 87 % is the linear-light figure). The assertions hold under both models, so the case does not rest on the choice. This is why aging by brightness was retired |
| FR-17 | TC-53 The shape says parent or child, never which status | Every status is drawn as the same filled disc — `unknown` excepted, which keeps an outline because it must carry the least ink of the six. Asserted over the silhouette enum: a build that gave `terminated` a shape of its own would fail this, which is the direction the rule now runs (**rewritten 2026-08-27**; it used to assert the opposite, and FR-17 says why) |
| FR-17 | TC-54 Every animated status has a static substitute | For each status with motion, a reduced-motion rendering exists. With the silhouettes gone this rests on the palette alone, which TC-49 … TC-51 measure |
| FR-55 | TC-103 The diagnosis reports four preconditions, each with one action | The four of `hook-setup.md` §5, in repair order, and **a row that holds offers nothing**. **TC-103b**: a missing helper is told rather than given a button, because reinstalling the app is not something the product can do. **TC-103c**: the entries row offers no repair while the helper is missing — setting up would write entries naming a program that is not there, and the preview refuses it anyway. **TC-103d**: an unreadable settings file offers only opening it (FR-53), and a file that is *not there yet* is not a failure at all. **TC-103e**: with nothing installed, the silence row offers no action of its own — the repair belongs to the row above, and "the single action" stops being single the moment it appears twice |
| FR-52, FR-55 | TC-104 A leftover event log is not evidence that anything is reporting | The event log outlives the entries that filled it: remove them and a report from a minute ago is still in the file. A row reading only the newest timestamp said the reporting was fine on the same screen as "none of them are there". **TC-104b**: the silence threshold is the core's `HOOK_QUIET_MS` and not a second copy, asserted on both sides of it, with the 30 minutes written out as a literal from `session-state-model.md` |
| FR-32 | TC-102 The tray icon shows the roll-up on the board's own ground | The centre of the icon is the indicator colour and the ground under it is `theme.board()` — the pair `ui-overlay.md` §3.3 measured. **TC-102b**: the tile has the board's edge colour all the way round, which is what separates it from a taskbar nobody measured anything against. **TC-102c**: `unknown` is a ring with a hole in it, not the dotted ring the board draws — at 16 px the dots resolve to a smudge that reads as a filled disc. **TC-102d**: no two statuses draw the same icon, in either theme. This does not re-measure the palette (TC-48 … TC-51 do); it checks that the icon uses it |
| FR-33 | TC-105 The port with a lock is the one chosen | The extension host listens on **several** ports and only one is Claude Code's; the chain took the first the process table gave it, so every click on that window's sessions was a silent no-op. Measured on a live machine: 54851, 58915, 59126, with the lock under 59126 ([ADR-0027](adr/0027-pick-the-editors-port-by-its-lock-file.md)). **TC-105b**: a port whose lock names no folder is passed over too. **TC-105c**: a host no lock claims yields nothing, and the raise ends as `NotFound` rather than as a wrong window |
| FR-33 | TC-107 The window no other session accounts for is the answer | An editor window with no folder open has no folder in its lock and none in its title (measured: `"メモを整理して - Visual Studio Code - Insiders"`), so it is named by elimination — the instance's other hosts account for all but one of its windows ([ADR-0028](adr/0028-name-the-last-window-by-elimination.md)). **TC-107b**: two unaccounted-for windows raise **neither**, because a wrong window is worse than a visible failure (L-7). **TC-107c**: none left is not an answer either, and must not fall through to "raise the first one". **TC-107d**: a lone window needs no elimination |
| FR-19 | TC-111 A session is named what its tab is named | The opening human prompt is read, because the editor's tab shows it until an `ai-title` exists ([ADR-0030](adr/0030-a-session-is-named-what-its-tab-is-named.md)). **TC-111b**: only a `user` record whose `origin.kind` is `human` — a tool result is a `user` record too, and its content is neither a prompt nor a name. **TC-111c**: the first line only, capped at 120 characters, and whitespace-only is no name at all. This is the one place the product builds text the user wrote, so what it will and will not read is asserted rather than described |
| FR-19 | TC-112 The order the board picks a name in is the editor's | Generated title, then the opening prompt, then the derived slug — the slug last because it is not a name anybody wrote, but above the workspace label because it tells two sessions of one workspace apart. **TC-112b**: a chosen name sits under the generated title, and the test **states that the order of those two is unmeasured** — every `nameSource` seen live was `derived`, so no recording shows what the tab does for a renamed session |
| FR-33 | TC-108 Each editor is addressed through its own URL scheme | Revealing a session means sending a URL to the editor ([ADR-0029](adr/0029-reveal-the-session-through-the-editors-url-handler.md)), and the scheme says which editor: an Insiders window sent a `vscode://` URL would hand it to a different application. **TC-108b**: an editor this build does not know is **not** guessed at. **TC-108c**: only an id-shaped string is pasted into the URL — it is about to be handed to another program. **TC-108d**: the editor's name comes from the lock that claimed the port, walked the same way the folder is (TC-105) |
| FR-33 | TC-101 Raising a window does not rearrange it | A real maximised window is created, raised, and asked whether it is still maximised. `SW_RESTORE` un-maximises, and was being sent to every window — so a click shrank the editor the user was working in. Skips with a printed reason where there is no desktop to create a window on |
| FR-18, FR-27 | TC-100 Every built-in command the front end calls is granted | The capability file lists `core:event:allow-listen` and `core:window:allow-start-dragging`, because the front end calls both. **TC-100b**: nothing is granted that no caller can be pointed at. **TC-100c**: both window labels are covered, including the setup window, which is created at runtime and is the kind that gets left off a list written earlier |

TC-48 … TC-52 are the numbers in `ui-overlay.md` §3.3 turned into assertions. They are
listed there as measured values, so the first run of these tests is also a check that the
document is telling the truth.

**They also carry more weight than they used to.** Until 2026-08-27 each status had a
silhouette of its own, so the palette had a second channel standing behind it. FR-17 now
leaves colour to do the work by itself, which means TC-49, TC-50 and TC-51 — the tier-A and
tier-B separations under three colour-vision simulations, and the greyscale floor — are no
longer belt *and* braces. They are the belt.

**TC-100 is a configuration case, and it exists because the configuration is what failed.**
Tauri v2 denies every built-in command that no capability grants, and the denial lands in the
front end where nothing is watching: the promise rejects, and the window carries on looking
correct. The board shipped a whole phase that way — it rendered once from `current_board`,
never received another `board` event, and could not be dragged. Commands the product defines
itself need no grant, which is why everything that looked like the hard part kept working and
the failure read as a rendering bug. Nothing in §7.2 caught it because none of §7.2 had been
executed.

### 7.2 Manual — needs a running board

Executed with **5+ sessions across 3+ VS Code windows**, which is the condition
[requirements.md](requirements.md) §10 already sets for acceptance.

> **These cases had never been executed**, and one bug lived in that gap for a whole phase:
> the board rendered once and then ignored every update, and the window could not be dragged
> (TC-100's note). TC-56, TC-61 and TC-65 would each have caught it on the first run. The
> automated case now guards the specific cause; the general lesson is the one §1 rule 3 is
> about — a checklist nobody runs is not coverage, and listing it as though it were is the
> failure mode this document is supposed to avoid.

| Req | Test case | Expectation |
|---|---|---|
| FR-18 | TC-55 Fold and unfold | Clicking a group header folds it to one row and unfolds it again, in one action each way. The board's top-left corner does not move — it grows and shrinks downwards |
| FR-18 | TC-56 A folded group still reports status | Fold a group, then drive one of its sessions to `awaiting_user`. The folded row's roll-up indicator changes and blinks **without unfolding**. This is the property that makes folding safe |
| FR-18 | TC-56a A one-session workspace is one row | A group with exactly one session is drawn as a single merged row: no header, no count, **no chevron**, and nothing to unfold |
| FR-18 | TC-56b Fold state is remembered per group | Fold two of four groups, quit, relaunch. The same two come back folded |
| FR-18 | TC-56c The height cap folds automatically | Exceed the configured row count. Groups fold, least urgent first, until it fits — and a group containing an `awaiting_user` session is **never** folded |
| FR-18 | TC-56e A fully folded board narrows | Fold every group. No session title is drawn anywhere, and the board's width drops to fit the longest workspace name, not 300. Unfolding any group returns it to 300 |
| FR-18 | TC-56f The width does not twitch | With everything folded, drive a session through several status changes and let its title change. **The width does not move.** It is recomputed only when the set of workspaces changes |
| FR-18 | TC-56g A merged row hides its title only while folded | A one-session workspace shows its title when any group is open and hides it when everything is folded. The full title is on hover in both states (FR-22) |
| FR-18 | TC-56h Resizing respects the snapped edge | Snap the board to the right screen edge, then unfold a group. It grows **leftward** and stays fully on screen. Repeat at the bottom edge |
| FR-18 | TC-56d Headers fold, rows raise | A click on a group header must **not** raise any window; a click on a session row or a merged row must **not** change any fold state |
| FR-23 | TC-57 A status change never reorders | Drive one session through `working` → `awaiting_user` → `idle` while watching a neighbouring group. No row changes its position |
| FR-23 | TC-58 A session ending moves only its own group | End the first session of the second group. Its siblings move up one row and **no group's order changes** |
| FR-23 | TC-60 Group order survives a restart | Restart the board. Group order is unchanged (it is persisted first-seen order, not alphabetical) |
| FR-24 | TC-61 Time is time-in-status | The column resets when the status changes, and does **not** show session age. Granularity coarsens `s` → `m` → `h` |
| FR-24 | TC-62 `limited` shows the reset clock time | The time column reads a clock time (`05:50`), not an elapsed count, and does not tick seconds |
| FR-26 | TC-63 Above a maximised editor | The board stays visible over a maximised and over a full-screen-borderless VS Code window |
| FR-26 | TC-64 Below a system-modal dialog | A UAC prompt covers the board. This is the one case where staying on top is wrong |
| FR-27 | TC-65 Drag by the body | Press-and-move anywhere on the background moves the board. There is no title bar to look for |
| FR-27 | TC-66 The empty board is still grabbable | With no sessions running the window is **140 × 30** — one row at the narrowest width — and can still be dragged |
| FR-28 | TC-67 Position, fold state and display persist | Move to a second display, fold two of four groups, quit, relaunch. All three come back. There is no density to switch — TC-56b covers the fold state per group |
| FR-28 | TC-68 A vanished display is survived | Persist a position on a second display, disconnect it, relaunch. The board appears fully on-screen on a display that exists |
| FR-28 | TC-113 A board on a live display is left alone | The rescue runs every few seconds and does nothing in the ordinary case. Asserted on both a single display and a two-display desk, because a board that crept back to a corner every six seconds would be a board fighting its owner |
| FR-28 | TC-114 A board on a vanished display comes back | The position the board held on a display that is no longer there is brought to the **nearest** live one — nearest rather than primary, so a board comes back roughly where its owner had it instead of being thrown to a screen they were not using. *Nearest* is the whole of what "no further than it has to" means here: once the rescue does act, the board lands **fully inside** that display rather than nudged until a grab's worth of it shows. The threshold decides **whether** to move it; it does not decide where it lands, and a board rescued to 95 % off-screen would be reachable and useless. This is TC-68 while the board is running rather than at its next start — the check used to happen once, at startup, and the display that goes away at 11am is a board nobody sees again that day |
| FR-28 | TC-115 A board straddling two displays is not rescued | Where the seam runs through it, each half on its own is larger than the grab patch. Reachability is measured per display rather than against the union, and this is the case that says why |
| FR-28 | TC-116 A board parked over an edge is not rescued | Half off the bottom, and half off the right. Someone keeping the board out of the way has not lost it. **TC-116b**: exactly a grab's worth showing is reachable and one pixel less is not — the two sit either side of the same line |
| FR-28 | TC-117 Placement uses the work area, not the display | A summoned board measures from the work area's own top-left, so a taskbar never has it. With no displays at all — every monitor asleep — nothing is moved to a coordinate invented out of nothing |
| FR-28 | TC-119 A resize puts back only what the resize pushed out | The board grows right and down from its top-left, and a board summoned to a corner has only the inset to grow into — so a board that was wholly on a display and is not after it grew comes back by exactly its overhang, on the axis that went over and no other. Reported from a desk with a 100 % display beside a 175 % one, where the same board is 75 % wider on one screen than the other. A board its owner parked over an edge was not put there by a resize and is not moved by one, which is TC-116's restraint carried through the second way the board can move; and a board grown larger than the display it is on goes to that display's corner rather than off the opposite edge |
| FR-32 | TC-123 A summoned board lands in the top-right | Inset from both edges of the work area, and reachable once it is there |
| FR-32 | TC-124 A display left of the origin gets its own corner | Negative coordinates are the second display's ordinary case, and the corner is that display's rather than the virtual desktop's |
| FR-32 | TC-125 A board larger than the display is still on it | The inset is a courtesy; being on screen is not. The corner wins |
| FR-32 | TC-126 What a click on the tray icon does | Hidden, visible-but-unreachable, and visible-and-reachable. The middle row is the defect this was written for: the click used to toggle visibility without touching the position, so "showing" a board on a display that no longer exists showed nothing — a no-op no user can tell from a dead icon |
| FR-32 | TC-127 The board says where it is | Hidden, off every display, or on display N — with the display showing the most of it being the one named. The three cannot be told apart by looking and have three different repairs, so the tray menu states which. **TC-127b**: the line is measured against the same edge as the rescue — a grab's worth showing, and one pixel less either way — because the two repeat the threshold rather than sharing it, and a board the rescue leaves alone while the menu calls it lost is the product contradicting itself. With every monitor asleep the answer is *lost*, not a display invented out of nothing; a display above the origin is named like one to the left; and a board split evenly across a seam names the same display every time, because a line that flipped while the board sat still would read as the board moving |
| FR-61 | TC-130 A cleared key is a decision, not a failure | Empty and whitespace both read as *cleared*, and the line the menu builds says "not set" without the dash it uses to report a problem. Someone who turned a shortcut off is not looking at a fault |
| FR-61 | TC-131 One unreadable key says nothing about the other | The defect this was written for: a key that would not parse returned before the other had been offered at all, so a typo in one setting silently disabled both. Each is read on its own, and the menu carries one line each |
| FR-61 | TC-132 What counts as a key | Four accelerators that parse and four that do not, on either side of the same line. A modifier is required — a bare letter would take that key from every program on the machine |
| FR-61 | TC-133 A key somebody else has reads differently from one nobody wanted | *Taken* names the reason, so the user knows it was not the board that failed, and still shows the key because it is what they chose; *cleared* has no key to show. One is somebody else's doing and one is their own, and a product that reported both the same way would be blaming itself for a collision |
| FR-61 | TC-134 A rebinding survives the file | A changed key and a cleared key both go through the settings file and come back as what they were. Clearing is a state the file has to be able to hold, not an absence it falls back to a default from |
| FR-61 | TC-135 A bare key is not one this will take | `M`, `F5`, `` ` `` and `Escape` all parse as good accelerators — measured — and registering one globally takes that key from every program on the machine. The rule that a shortcut needs a modifier therefore lives where the key is offered to the operating system, not only in the window where it is chosen: the settings file can be edited by hand, copied between machines, or written by an older build, and none of those go past the setup window |
| FR-61 | TC-136 The other shortcut having it is not another application having it | Both rows set to the same key is this product failing to notice, and the second registration fails. Reporting that as *taken* would send the user hunting through their own machine for the application holding a key the board is holding itself, so it is its own state with its own sentence |
| FR-60, FR-61 | TC-137 A flag asks for the same screen from either direction | One board per machine sends a second launch to the running one, and the request it carries is its arguments. Dropping them made `--shortcuts` — the documented way to reach that screen — summon the board and open nothing, for everybody whose board was already open. Found on the machine, not in a test: the flags worked while no board was running, which is the state a developer is least often in. Both paths now read one mapping |
| FR-29 | TC-69 Mixed DPI | Drag from a 100 % display to a 150 % one. Indicators, text and the hit areas all scale; nothing is drawn at the old scale |
| FR-29 | TC-70 Every scaling factor | Readable at 100 %, 125 %, 150 % and 200 % |
| FR-30 | TC-71 Updates never steal focus | Type continuously in an editor while sessions change status, including a blink and a raise-failure marker. **No keystroke is lost and the caret never leaves the editor** |
| FR-30 | TC-72 The board never becomes foreground on its own | Poll the foreground window through a status storm. It is never the board |
| FR-33 | TC-73 Raise reaches the right window | From every one of the 3+ windows, click each session; the window that comes forward has that session's workspace open. Passing needs it to work from a board that has not been clicked first, since that is the refused case ([ADR-0009](adr/0009-map-a-session-to-its-window.md)) |
| FR-33 | TC-74 Same folder in two windows | Duplicate a workspace into a second window. Raising **either** passes — L-1 is stated in the requirement, so this case asserts "no crash, no wrong-workspace window", not "the exact window" |
| FR-33 | TC-75 A failed raise is visible | Force the failure (a customised `window.title`, or close the target between click and raise). The indicator shows the hollow outline for 2 s and the tooltip explains. **A silent no-op fails this test** |
| FR-33 | TC-76 A successful raise is acknowledged | The 150 ms ring flash appears, and does not appear when the raise failed |
| FR-33 | TC-77 Click versus drag | A press that moves ≤ 4 px raises; a press that moves more drags and raises nothing. Test both starting **on** an indicator |
| FR-33 | TC-78 The hit area is the whole pitch | Click 3 px to the left of an indicator's drawn edge, still inside its 16 px pitch. It raises |
| FR-34 | TC-79 Off by default | A fresh profile emits no sound and no flash on any transition |
| FR-34 | TC-80 Fires on the two transitions, and only those | Enabled: `→ awaiting_user` and `→ terminated` alert. `→ idle`, `→ working` and `→ limited` do not |
| FR-34 | TC-81 The alert does not steal focus either | TC-71 while alerts are enabled |
| FR-55 | TC-98 The current mode is visible | With the entries installed and a session reporting, the setup window's standing line and the board's tooltip both say **exact**; with them removed, both say the status is worked out from a pause. **Partial**: FR-55 asks for the board's *menu*, which does not exist yet, and for a diagnosis view, which is deferred (requirements.md §11) |
| FR-52 | TC-99 The board stops claiming exactness it has lost | With the entries installed, stop the helper from running (rename it) and leave the board for longer than `T_hook_quiet`. The mode changes to *worked out from a pause* **without the entries being removed**, and the sessions carry on being classified by inference |

**TC-59 no longer exists.** It tested that a workspace reclaimed its slot within 10 minutes,
which was a mechanism of the minimal density's fixed-slot scheme. That density is gone
([ADR-0012](adr/0012-one-density-that-folds.md)) and so is the mechanism. The number is left
retired rather than reused, so an old reference resolves to nothing instead of to the wrong
case.

### 7.3 Still not covered here, and why

- **The hook *setup* flow** — FR-41 … FR-51, FR-53, FR-54. In v1 scope, but it is a
  settings-editing flow rather than core logic or UI, and it needs its own plan. Narrowed
  from the previous revision: the *status* half of the hook layer (FR-36, FR-52, and the
  right-hand column of the state model §3) is now covered in §5.6. Four of the five events
  have still never been observed (U-5), which §5.6 states in each row that depends on it.
- **NFR-03 (footprint), NFR-06 (availability), NFR-08 (no network), NFR-09 (transparency)** —
  properties of the running application, measured against a build.
- **End-to-end latency** — TC-47 covers only the core's share of NFR-01.
- **Whether the design works for anyone but its author.** TC-55 … TC-81 check that the board
  does what `ui-overlay.md` says. They cannot check that what it says is right — in
  particular the three mechanisms of
  [ADR-0011](adr/0011-answer-which-session-by-position-and-hover.md) are reasoned, not tested,
  and that ADR says so.

## 8. Coverage

Coverage by requirement, stated honestly — "has a row in this document" is not the same as
"can be tested today":

| Area | Requirements | Covered by |
|---|---|---|
| Discovery | FR-01 … FR-07 | TC-01 … TC-07 |
| Status | FR-08 … FR-15, FR-56 … FR-59 | TC-09 … TC-28 |
| Title fitting | FR-19 … FR-22 | TC-29 … TC-36 |
| Resilience | FR-38, FR-39, FR-40 | TC-08, TC-37 … TC-39 |
| Non-functional (core) | NFR-01, NFR-02, NFR-05, NFR-07, NFR-10, NFR-11 | TC-40 … TC-47 |
| Presentation, automated | FR-16, FR-17 | TC-48 … TC-54 — palette data, no window needed |
| Tray icon, automated | FR-32 | TC-102 … TC-102d — the composited pixels, no shell needed |
| Window placement, automated | FR-28, FR-32 | TC-113 … TC-117 and TC-123 … TC-127 — a position, a size and a list of work areas, so the display that goes away never has to |
| Shortcuts, automated | FR-61 | TC-130 … TC-137 — what an accelerator reads as, and every sentence the menu builds from the result. Whether a key is *free* is not among them: that is a fact about the machine the tests run on, and the product answers it by asking rather than by reasoning |
| One board, automated | FR-60 | TC-128 and TC-129 — the real binary launched twice. Whether a second process stops itself is a fact about two processes, and no function can be asked it. **How** it stopped is asserted as well as that it stopped: any crash on startup would also exit, draw nothing and leave the first board alone, and the case would then pass while proving only that this machine cannot run two boards. TC-129 watches for a second window **while** the second process runs rather than after it has gone, because a board that flickered up and withdrew would leave nothing behind to find. Sampling cannot prove a window never existed, and it is not asked to: the failure the guard exists for is a board that *stays*, and a Tauri window once opened is there until its process ends. Skips with a printed reason where there is no desktop, and **before launching anything** where a board is already running — a probe started beside a developer's own board becomes a client and summons it, and running the tests should not move someone's windows |
| Front-end permissions, automated | FR-18, FR-27 | TC-100 … TC-100c — the capability file against the calls the front end makes |
| Raising, automated | FR-33 | TC-105 … TC-105c on the lock files, TC-107 … TC-107d on the titles and TC-108 … TC-108d on the URL; TC-101, TC-106 and TC-106b on a real window, which is the only way to test what Windows does with a flag |
| **Deliberately not automated** | FR-33's session reveal | Whether the editor honours the URL cannot be asserted without the editor. It was established by measurement (ADR-0029) and is guarded by the four conditions in `app::reveal_session`, each of which fails into doing nothing |
| Presentation and window, manual | FR-18, FR-23, FR-24, FR-26 … FR-30 | TC-55 … TC-58, TC-60 … TC-72 (**TC-59 is retired** — see below) |
| Interaction, manual | FR-33, FR-34 | TC-73 … TC-81 |
| Hook mode, automated | FR-36, FR-52 | TC-82 … TC-99 — the status half of the hook layer |
| Hook mode, manual | FR-52 | TC-98, TC-99 — needs a running board and a real session |
| Diagnosis, automated | FR-55 | TC-103 … TC-104b — `setup::diagnose` is a function of `Facts`, so none of the four failures has to be produced on the machine running the tests |
| **Deliberately elsewhere** | The hook setup flow (FR-41 … FR-51, FR-53, FR-54) | §7.3 — v1 scope, but not core and not UI; it needs its own plan |

**One requirement is not actually covered.** TC-19 (FR-14, an unrecovered error ending a
turn) is specified but **cannot be written**, because U-1 has no measured sample. Listing a
row for it is bookkeeping, not coverage — it is a tracked gap, and the honest count is:

| | |
|---|---|
| v1 core requirements with a writable test case | all except FR-14's error half |
| v1 core requirements with a row but **no** writable test | **1** (FR-14 error half) |
| v1 UI requirements with an **automated** test | 2 (FR-16, FR-17) |
| v1 UI requirements testable only **by hand, against a build** | 11 (FR-18, FR-23, FR-24, FR-26 … FR-30, FR-33, FR-34) |

It was two. FR-15 joined the covered side when the usage limit turned out to be measurable
after all (§6.3) — the requirement never changed, only what was known about the data.

The eleven manual rows are not a gap in the same sense: they have criteria and they can be
executed, they simply need a window and a person. What they cannot do is run in CI, so a UI
regression is caught by someone following §7.2, not by a build failing. That is the cost of
the requirements being about a window, and it is worth stating rather than disguising with a
screenshot-diff harness that would test the screenshot rather than the requirement.

The gap does not mean the requirement is wrong. FR-14's error case is rare rather than
absent — three and a half months of one developer's transcripts is not evidence that a class
of failure does not exist, only that it did not happen to occur. The test waits for a
sample; the requirement stands.
