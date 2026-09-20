// This module is compiled into each integration-test binary separately, so a helper used by
// one of them is dead code in the others. That is the shape of Cargo's test layout, not a
// smell worth chasing.
#![allow(dead_code)]

//! Reaching the recorded fixtures from a test.
//!
//! `tests/fixtures/` sits at the repository root, not under this crate: the recordings are
//! shared data rather than one crate's private files
//! ([ADR-0016](../../../../docs/adr/0016-lay-the-code-out-as-a-cargo-workspace.md)). That means
//! a path that crosses a crate boundary, and this module is the one place it is written —
//! resolved from `CARGO_MANIFEST_DIR` so it does not depend on the working directory.

use std::path::PathBuf;

/// Absolute path to `tests/fixtures/<name>`.
pub fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Reads a fixture, failing loudly: a missing recording is a broken checkout, not a case to
/// degrade around.
pub fn read(name: &str) -> String {
    let p = path(name);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", p.display()))
}

/// The lines of a `.jsonl` fixture, paired with the **byte offset** each one starts at.
///
/// The offset is the ordering key the state machine uses, so it is carried from the start
/// rather than reconstructed later ([ADR-0006](../../../../docs/adr/0006-order-events-by-append-position.md)).
pub fn lines(name: &str) -> Vec<(u64, String)> {
    let text = read(name);
    let mut offset = 0u64;
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        let bytes = line.len() as u64;
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if !trimmed.is_empty() {
            out.push((offset, trimmed.to_owned()));
        }
        offset += bytes;
    }
    out
}

/// The registry fixture's entries, as the JSON text of one `sessions/<pid>.json` each.
///
/// The file bundles seventeen of them for convenience; on disk they are separate files, and
/// the core parses them one at a time, so the bundling is undone here rather than modelled.
pub fn registry_entries() -> Vec<(bool, String)> {
    let text = read("session-registry.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("registry fixture is JSON");
    doc["entries"]
        .as_array()
        .expect("registry fixture has entries")
        .iter()
        .map(|e| {
            // `_note_live` is fixture metadata, not a field Claude Code writes.
            let live = e["_note_live"].as_bool().unwrap_or(false);
            let mut entry = e.clone();
            entry
                .as_object_mut()
                .expect("entry is an object")
                .remove("_note_live");
            (live, entry.to_string())
        })
        .collect()
}

/// One observation from a recording, ready to be folded into a machine.
pub struct Replay {
    pub observations: Vec<mcv_core::event::Observation>,
}

/// Replays a plain transcript: each line becomes a `Record` observation, seen at the moment
/// the record says it was written.
///
/// A record with no timestamp of its own is seen at the same moment as the last one that had
/// a timestamp — the observer was there either way, and inventing a later instant would put
/// time into the stream that the recording does not contain.
pub fn replay_transcript(name: &str) -> Replay {
    use mcv_core::event::{Event, Observation};
    use mcv_core::record::parse_line;

    let entries: Vec<(u64, mcv_core::record::Entry)> = lines(name)
        .into_iter()
        .map(|(offset, line)| {
            let entry =
                parse_line(&line, offset).unwrap_or_else(|e| panic!("{name} at {offset}: {e:?}"));
            (offset, entry)
        })
        .collect();

    // Records *before* the first timestamped one are seen at that first instant, not at the
    // epoch. Anchoring at zero would put fifty years of silence in front of the recording —
    // a gap the observer never lived through, and one `with_ticks` would dutifully fill.
    let mut at = entries
        .iter()
        .find_map(|(_, entry)| entry.meta.at)
        .unwrap_or_else(|| mcv_core::time::Timestamp::from_millis(0));

    let mut observations = Vec::new();
    for (offset, entry) in entries {
        if let Some(stamp) = entry.meta.at {
            at = stamp;
        }
        observations.push(Observation::new(
            at,
            Event::Record {
                offset,
                entry: Box::new(entry),
            },
        ));
    }
    Replay { observations }
}

/// Replays a **merged observation stream** — `prompt-observed.jsonl` and
/// `prompt-denied.jsonl`, the two recordings that are not plain transcripts.
///
/// Each line carries `t` and a `src` of `transcript` or `process`, which is the vocabulary
/// `mcv_core::event` was shaped to hold. The process events are the ones the state machine's
/// rule 3 turns on, and they were recorded across a real permission prompt.
pub fn replay_stream(name: &str) -> Replay {
    use mcv_core::event::{Event, Observation};
    use mcv_core::record::parse_line;
    use mcv_core::time::Timestamp;

    let mut observations = Vec::new();
    for (offset, line) in lines(name) {
        let envelope: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("{name}: {e}"));
        let at = Timestamp::parse_iso8601(envelope["t"].as_str().expect("every line carries t"))
            .expect("the envelope's stamp is the recorded format");

        let what = match envelope["src"].as_str() {
            Some("transcript") => {
                let record = envelope["record"].to_string();
                let entry = parse_line(&record, offset).expect("the embedded record parses");
                Event::Record {
                    offset,
                    entry: Box::new(entry),
                }
            }
            Some("process") => match envelope["event"].as_str() {
                Some("no_process_for_pending_call") => Event::NoProcessForPendingCall,
                Some("tool_process_started") => Event::ToolProcessStarted,
                Some("tool_process_exited") => Event::ToolProcessExited,
                other => panic!("{name}: unknown process event {other:?}"),
            },
            other => panic!("{name}: unknown src {other:?}"),
        };
        observations.push(Observation::new(at, what));
    }
    Replay { observations }
}

impl Replay {
    /// Fills the gaps between observations with ticks.
    ///
    /// The recordings contain long silences — 44 seconds of an unanswered prompt, 4,737 of a
    /// pending call — and the machine is forbidden from noticing time passing on its own
    /// (ADR-0019). The observer is what notices, so the harness supplies the ticks, exactly
    /// as `docs/test-plan.md` §5.2 says it must.
    #[must_use]
    pub fn with_ticks(self, every_ms: i64) -> Self {
        use mcv_core::event::{Event, Observation};
        use mcv_core::time::Timestamp;

        let mut out: Vec<Observation> = Vec::new();
        let mut previous: Option<i64> = None;
        for observation in self.observations {
            let now = observation.at.as_millis();
            if let Some(mut cursor) = previous {
                cursor += every_ms;
                while cursor < now {
                    out.push(Observation::new(
                        Timestamp::from_millis(cursor),
                        Event::Tick,
                    ));
                    cursor += every_ms;
                }
            }
            previous = Some(now);
            out.push(observation);
        }
        Self { observations: out }
    }

    /// Appends ticks past the end of the recording, for cases about what happens while a
    /// call stays unanswered.
    #[must_use]
    pub fn then_ticks(mut self, count: usize, every_ms: i64) -> Self {
        use mcv_core::event::{Event, Observation};
        use mcv_core::time::Timestamp;

        let mut cursor = self.observations.last().map_or(0, |o| o.at.as_millis());
        for _ in 0..count {
            cursor += every_ms;
            self.observations.push(Observation::new(
                Timestamp::from_millis(cursor),
                Event::Tick,
            ));
        }
        self
    }
}
