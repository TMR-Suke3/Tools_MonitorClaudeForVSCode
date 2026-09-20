//! TC-37 … TC-39, TC-42, TC-43, TC-46 — reading a transcript (FR-38, FR-39, NFR-05, NFR-07, NFR-11).
//!
//! Every case runs on recordings in `tests/fixtures/`, never on hand-written JSON: a
//! hand-written fixture encodes the same assumption the parser does, and the test passes
//! without proving anything (`docs/test-plan.md` §1, rule 1). Where a case needs damage —
//! a truncated line, a corrupt one — it **derives** it from a recording by cutting it, which
//! is what rule 1 allows and what TC-38 asks for in as many words.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::record::{Block, Entry, Record, StopReason, parse_line};
use mcv_core::time::Timestamp;

/// Every recording that is a plain transcript — all of them.
///
/// The two `prompt-*` files are the only exclusions, and they are excluded for a reason
/// rather than by oversight: they are **merged observation streams**, transcript records
/// interleaved with process-table transitions under a `{t, src, …}` envelope, so they are
/// not lines this parser is given. `tests/fixtures/README.md` says so.
///
/// The list was shorter than this and claimed to be complete. Three recordings were missing,
/// which quietly narrowed TC-42, TC-43 and TC-45 — the "no content retained" guarantee least
/// of all wants a corpus that is smaller than it says it is.
const TRANSCRIPTS: [&str; 15] = [
    "turn-normal.jsonl",
    "api-error-retrying.jsonl",
    "api-error-late-append.jsonl",
    "api-error-connection.jsonl",
    "compaction.jsonl",
    "subagent-parent-pending.jsonl",
    "subagent-child.jsonl",
    "pending-interactive.jsonl",
    "pending-ordinary-long.jsonl",
    "title-change.jsonl",
    "tail-untimestamped.jsonl",
    "unknown-record-types.jsonl",
    "entrypoint-desktop.jsonl",
    "streaming-null-stop-reason.jsonl",
    "usage-limit-rejected.jsonl",
];

/// Guards the list above against the recordings growing without it: every `.jsonl` in the
/// fixture directory is either in `TRANSCRIPTS` or is one of the two observation streams.
#[test]
fn every_recording_is_accounted_for() {
    const STREAMS: [&str; 2] = ["prompt-observed.jsonl", "prompt-denied.jsonl"];

    let dir = fixtures::path(".");
    let mut found: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            name.ends_with(".jsonl").then_some(name)
        })
        .collect();
    found.sort();

    for name in &found {
        assert!(
            TRANSCRIPTS.contains(&name.as_str()) || STREAMS.contains(&name.as_str()),
            "{name} is a recording this suite never reads; add it to TRANSCRIPTS or say why it is not a transcript",
        );
    }
    assert_eq!(
        found.len(),
        TRANSCRIPTS.len() + STREAMS.len(),
        "the fixture directory and the lists in this file disagree: {found:?}",
    );
}

fn parsed(name: &str) -> Vec<(u64, Entry)> {
    fixtures::lines(name)
        .into_iter()
        .map(|(offset, line)| {
            let entry = parse_line(&line, offset)
                .unwrap_or_else(|e| panic!("{name} at {offset} does not parse: {e:?}"));
            (offset, entry)
        })
        .collect()
}

// ---------------------------------------------------------------- FR-38

/// TC-37 — unknown record types are skipped silently, and the records around them survive.
#[test]
fn tc_37_unknown_record_types_are_skipped() {
    let entries = parsed("unknown-record-types.jsonl");

    let ignored: Vec<&Option<String>> = entries
        .iter()
        .filter_map(|(_, e)| match &e.record {
            Record::Ignored { kind } => Some(kind),
            _ => None,
        })
        .collect();
    assert!(
        ignored.iter().any(|k| k.as_deref() == Some("atis-latch")),
        "the recording contains atis-latch, a type no specification mentions",
    );

    // Silently: skipping is not a parse failure, so nothing in this fixture errored — the
    // call above would have panicked if it had.
    let understood = entries
        .iter()
        .filter(|(_, e)| !matches!(e.record, Record::Ignored { .. }))
        .count();
    assert!(
        understood > 0,
        "the records around the unknown one are still read",
    );
}

/// TC-38 — malformed and partial records are tolerated.
///
/// Derived from `turn-normal.jsonl` by cutting it: a line truncated mid-JSON, and a line
/// corrupted in place. Neither may kill the reader, and neither may swallow its neighbours.
#[test]
fn tc_38_malformed_and_partial_records_are_tolerated() {
    let lines = fixtures::lines("turn-normal.jsonl");
    assert!(lines.len() >= 3, "the recording is long enough to damage");

    let truncated = &lines[1].1[..lines[1].1.len() / 2];
    assert!(
        parse_line(truncated, lines[1].0).is_err(),
        "half a record is not a record; the reader must not accept it as one",
    );

    let corrupt = lines[2].1.replacen('{', "{{", 1);
    assert!(parse_line(&corrupt, lines[2].0).is_err());

    // The neighbours are unaffected: a bad line costs exactly itself.
    assert!(parse_line(&lines[0].1, lines[0].0).is_ok());
    assert!(parse_line(&lines[3].1, lines[3].0).is_ok());
}

/// TC-39 — unrecognised data degrades visibly: one report, naming where it happened.
#[test]
fn tc_39_unparseable_lines_are_reported_once_and_located() {
    let lines = fixtures::lines("turn-normal.jsonl");
    let (offset, line) = &lines[1];
    let damaged = line.replacen('{', "", 1);

    let error = parse_line(&damaged, *offset).expect_err("this line cannot be read");
    assert_eq!(
        error.offset, *offset,
        "the report points at the byte the line started on",
    );
    assert!(
        !error.reason.is_empty(),
        "a silent failure is what FR-39 forbids",
    );
}

// ---------------------------------------------------------------- NFR-05

/// TC-42 — the observed Claude Code version is captured, per session.
#[test]
fn tc_42_the_observed_version_is_recorded() {
    let mut seen = 0;
    for name in TRANSCRIPTS {
        for (_, entry) in parsed(name) {
            if let Some(version) = &entry.meta.version {
                assert!(
                    !version.is_empty(),
                    "{name}: an empty version is not a version"
                );
                seen += 1;
            }
        }
    }
    assert!(
        seen > 0,
        "the recordings carry versions and they must be kept"
    );
}

// ---------------------------------------------------------------- NFR-07

/// TC-43 — no conversation content is ever retained.
///
/// The check walks the **recording** for the fields that carry content, then asserts none of
/// those strings survives into the parsed model. It works from the raw JSON rather than from
/// a list of field names in the parser, so a field the parser starts reading by accident
/// fails this test rather than passing it.
///
/// Named explicitly by the test plan: `queue-operation.content` is a full prompt and
/// `atis-latch.atis` is a credential. Both must be dropped **unread**.
#[test]
fn tc_43_no_content_is_ever_retained() {
    let mut checked = 0;
    for name in TRANSCRIPTS {
        for (offset, line) in fixtures::lines(name) {
            let raw: serde_json::Value = serde_json::from_str(&line).expect("fixture is JSON");
            let entry = parse_line(&line, offset).expect("fixture parses");
            let retained = format!("{entry:?}");

            for secret in content_strings(&raw) {
                // Short strings collide with enum names and ids by chance; the leak this
                // guards against is prose, and prose is not four characters long.
                if secret.chars().count() < 8 {
                    continue;
                }
                assert!(
                    !retained.contains(&secret),
                    "{name} at {offset}: content reached the retained model: {secret:?}",
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "the recordings are supposed to contain content worth not keeping",
    );
}

/// Every string in a record that carries conversation content, a prompt, or a credential.
///
/// Two rules, because content hides in two ways. Most of it sits directly under a known key
/// as a string — `text`, `thinking`, `error.message`, `queue-operation.content`,
/// `atis-latch.atis`. A tool's `input`, though, is an object whose keys are the tool's own
/// (`command`, `file_path`, `prompt`, …), so everything beneath it counts.
///
/// The distinction matters: a first version of this walker treated every string under a
/// `content` array as content and flagged `toolu_…` **tool-use ids**, which the recordings
/// preserve on purpose and which the state machine needs to join a call to its result.
fn content_strings(value: &serde_json::Value) -> Vec<String> {
    /// Content when the value at this key is a string.
    const SHALLOW: [&str; 7] = [
        "text",
        "thinking",
        "content",
        "atis",
        "message",
        "formatted",
        "summary",
    ];
    /// Content all the way down, whatever the keys below are called.
    const DEEP: [&str; 1] = ["input"];

    let mut out = Vec::new();
    walk(value, false, &mut out, &SHALLOW, &DEEP);
    out
}

fn walk(
    value: &serde_json::Value,
    deep: bool,
    out: &mut Vec<String>,
    shallow_keys: &[&str],
    deep_keys: &[&str],
) {
    match value {
        serde_json::Value::String(s) if deep => out.push(s.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                walk(item, deep, out, shallow_keys, deep_keys);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if let serde_json::Value::String(s) = child {
                    if deep || shallow_keys.contains(&key.as_str()) {
                        out.push(s.clone());
                    }
                    continue;
                }
                let descend = deep || deep_keys.contains(&key.as_str());
                walk(child, descend, out, shallow_keys, deep_keys);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- NFR-11

/// TC-46 — the ordering key is append position, not `timestamp`.
///
/// `api-error-late-append.jsonl` is the recording that proves it matters: the two error
/// records were appended **after** the turn they belong to had ended, carrying their
/// original earlier stamps. Sorting by timestamp reorders the file, and the reordering is
/// the wrong answer ([ADR-0006](../../../docs/adr/0006-order-events-by-append-position.md)).
#[test]
fn tc_46_the_ordering_key_is_append_position() {
    let entries = parsed("api-error-late-append.jsonl");

    let offsets: Vec<u64> = entries.iter().map(|(o, _)| *o).collect();
    let mut sorted = offsets.clone();
    sorted.sort_unstable();
    assert_eq!(offsets, sorted, "file order is already offset order");

    let stamped: Vec<(u64, Timestamp)> = entries
        .iter()
        .filter_map(|(o, e)| e.meta.at.map(|at| (*o, at)))
        .collect();
    let mut by_time = stamped.clone();
    by_time.sort_by_key(|(_, at)| *at);
    assert_ne!(
        stamped, by_time,
        "this recording is kept precisely because timestamp order differs from file order; \
         if they agree, the fixture is not the one TC-46 needs",
    );
}

/// TC-45, in part — replaying a recording twice yields the same result, and no case in this
/// file consults the clock or the filesystem for anything but the fixture itself.
#[test]
fn tc_45_parsing_is_deterministic() {
    for name in TRANSCRIPTS {
        assert_eq!(
            parsed(name),
            parsed(name),
            "{name} parsed differently twice"
        );
    }
}

/// Supporting — the shapes the state machine will key on are actually being recovered, so a
/// parser that silently produced empty records could not pass the cases above by default.
#[test]
fn the_signals_the_state_machine_needs_are_recovered() {
    let entries = parsed("turn-normal.jsonl");

    assert!(
        entries.iter().any(|(_, e)| matches!(
            &e.record,
            Record::Assistant(m) if m.stop_reason == StopReason::EndTurn
        )),
        "a turn ends structurally, on a stop reason",
    );
    // Ids and names, not just the presence of a block: an empty id would still satisfy
    // `is_some()`, and a mutation audit found exactly that gap.
    let calls: Vec<(String, String)> = entries
        .iter()
        .filter_map(|(_, e)| match &e.record {
            Record::Assistant(m) => Some(m.tool_uses()),
            _ => None,
        })
        .flatten()
        .map(|(id, name)| (id.to_owned(), name.to_owned()))
        .collect();
    assert!(!calls.is_empty(), "the recording contains tool calls");
    for (id, name) in &calls {
        assert!(
            !id.is_empty(),
            "a tool call with no id cannot be joined to its result"
        );
        assert!(
            !name.is_empty(),
            "the tool's name is what FR-56's rules key on"
        );
    }

    let results: Vec<(String, bool)> = entries
        .iter()
        .filter_map(|(_, e)| match &e.record {
            Record::User(m) => Some(m.tool_results()),
            _ => None,
        })
        .flatten()
        .map(|(id, is_error)| (id.to_owned(), is_error))
        .collect();
    assert!(!results.is_empty(), "the recording contains tool results");
    for (id, _) in &results {
        assert!(
            calls.iter().any(|(call_id, _)| call_id == id),
            "result {id} answers a call that is not in the recording; the join is what the pending-call rules are built on",
        );
    }
    assert!(
        entries
            .iter()
            .any(|(_, e)| matches!(&e.record, Record::Assistant(m)
                if m.blocks.contains(&Block::Thinking))),
        "thinking blocks are recognised as blocks, without their text",
    );
}

/// Supporting — the usage limit is decidable from structure alone (FR-15), with no wording.
#[test]
fn the_usage_limit_is_structural() {
    let entries = parsed("usage-limit-rejected.jsonl");
    let quota = entries
        .iter()
        .find_map(|(_, e)| e.quota.as_ref())
        .expect("the recording is a session cut off by the five-hour limit");

    assert!(quota.is_rejected());
    assert_eq!(quota.rate_limit_type.as_deref(), Some("five_hour"));
    assert!(
        quota.resets_at.is_some(),
        "the reset time is what the board shows"
    );
}
