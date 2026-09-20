# Fixtures

Recorded input for the core tests. See [docs/test-plan.md](../../docs/test-plan.md) for
which requirement each one defends and what status sequence it must produce.

## Where these come from

Every `.jsonl` here is a slice of a **real** Claude Code transcript, taken from the author's
machine on 2026-08-25 (78 transcripts, 19,392 records, versions 2.1.138 – 2.1.241), then
anonymised. None of them was written by hand — a hand-written fixture would bake in the same
assumption the implementation makes, and the test would pass without proving anything.

`titles.json` is a partial exception, and says so in its own `note` field: the **widths** are
measured from 98 real session titles, but the text is synthesised character-by-character to
match each original's length and rendered width exactly. Rows marked `"derived": true` are
not measured at all — they cover grapheme-cluster cases (ZWJ emoji, combining marks,
surrogate pairs, RTL) that do not occur in the corpus.

`session-registry.json` reproduces the measured shape and the measured live/stale ratio
(3 live of 17) with synthetic ids and paths.

`usage-limit-rejected.jsonl` was cut later, from the same machine once the corpus had grown
to 84 transcripts. It is the only recording of a **usage limit** — six exist in total, all
identical in shape — and it covers both halves of FR-15 in one slice: the rejection, the
seven minutes of silence that follow, and the turn that resumes afterwards with nothing
marking the recovery.

## What was removed

Anonymisation works from an **allow-list**: any field not explicitly named is dropped. A
deny-list was tried first and leaked absolute paths nested inside `file-history-snapshot`
records.

Removed: all message text and thinking, tool inputs, tool results, `queue-operation.content`
(which holds the full prompt), `last-prompt`, `atis-latch.atis` (a credential-shaped token),
file-history payloads, `usage`, `diagnostics`. Replaced: `cwd`, `gitBranch`, `aiTitle`, and
all ids. Never read in the first place: `authToken`, `messagingSocketPath`, and the
`sessions/*.key` files.

## What was deliberately kept

The fixtures must stay **equivalent as state-machine input**, so these are byte-for-byte
what was recorded:

record order · `type` · `subtype` · `stop_reason` · `isSidechain` · `entrypoint` ·
`version` · `retryInMs` / `retryAttempt` / `maxRetries` · `source` · `operation` ·
tool names · content-block types · `is_error` · `compactMetadata` numbers ·
`quotaLimits` (its enums, flags, and `resetsAt`)

**Timestamps are shifted by one global constant.** The real ones recorded when the author
happened to be at the keyboard (02:21, 23:32, …), which no test needs. The offset preserves
every inter-record gap to the millisecond — the measured 2,275 s `AskUserQuestion` wait, the
4,737 s `PowerShell` wait, the 354 s `Agent` dispatch, the 155,638 ms compaction — along
with the record order. `procStart` and `startedAt` in `session-registry.json` are synthetic
values of the right shape and magnitude, for the same reason.

Note that timestamps are **not** monotonic, and that is intentional: in the real corpus 56
of 78 files contain records written out of timestamp order, and the shift preserves every
inversion. `api-error-late-append.jsonl` exists specifically to preserve that.

`quotaLimits.resetsAt` is unix **seconds**, not an ISO string, and is shifted by the same
offset as the records around it — so the measured distance from the rejection to the reset
(5.7 minutes) survives intact. That distance is the point of the fixture: across the six
real occurrences it ranged from 5.4 minutes to 3.6 hours, so it can only be read, never
computed.

**Anything derived from a timestamp must use deltas, never the absolute date.** A test that
hard-codes a date is testing the anonymiser, not the product.

## `prompt-observed.jsonl` is a different shape

Every other fixture is a slice of one transcript. This one is the **merged observation
stream** that NFR-11 actually describes: transcript records *and* process-table transitions,
in the order an observer saw them. Each line carries a `src` of `transcript` or `process`.

It was recorded across a real permission prompt, deliberately left unanswered for 44 seconds
in manual permission mode. The transcript's own clock could not be used to align the two
sources — records are appended late and carry earlier timestamps — so the alignment comes
from the measured durations instead: 58.2 s of pending, of which 13.9 s had a process.

## Adding a fixture

1. Cut it from a real recording. Do not write one by hand.
2. Run it through the anonymiser, then through the leak check — which decodes each record
   as JSON before matching, because a plain `grep` for `C:\\Users` does not match the
   JSON-escaped `C:\\\\Users` that actually appears in the file.
3. Add its row to the inventory in `docs/test-plan.md` with the requirement it serves.
