//! TC-40, TC-41 — following a transcript by byte offset (NFR-02).
//!
//! These are the cases that cannot run in the core, because they are about a real file
//! changing under a reader
//! ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)
//! says so in as many words: purity in the core concentrates the untestable-without-a-disk
//! part here rather than removing it).
//!
//! Each case builds its file from a **recording** — `turn-normal.jsonl`, replayed onto disk
//! a few records at a time — so the bytes being appended are the bytes Claude Code actually
//! wrote (`docs/test-plan.md` §1, rule 1).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use mcv_board::watch::tail::Tail;

/// The recording, as whole lines with their terminators, ready to be appended in pieces.
fn recorded_lines() -> Vec<String> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/turn-normal.jsonl");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    text.split_inclusive('\n')
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// A scratch file that removes itself, named after the case using it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(case: &str) -> Self {
        let dir = std::env::temp_dir().join("mcv-tail-tests");
        fs::create_dir_all(&dir).expect("scratch directory");
        let path = dir.join(format!("{case}-{}.jsonl", std::process::id()));
        let _ = fs::remove_file(&path);
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn append(&self, text: &str) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
            .expect("open scratch file");
        f.write_all(text.as_bytes()).expect("append");
    }

    fn write(&self, text: &str) {
        fs::write(&self.0, text).expect("rewrite scratch file");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// TC-40 — reading is incremental: a poll reads the appended bytes and nothing else.
#[test]
fn tc_40_reading_is_incremental() {
    let lines = recorded_lines();
    assert!(
        lines.len() >= 4,
        "the recording is long enough to append in pieces"
    );
    let scratch = Scratch::new("tc40");

    let first: String = lines[..2].concat();
    scratch.append(&first);
    let mut tail = Tail::new(scratch.path());

    let start = tail.poll().expect("first poll");
    assert_eq!(start.lines.len(), 2);
    assert_eq!(
        start.bytes_read,
        first.len() as u64,
        "the first poll reads the file that exists",
    );

    let appended: String = lines[2..4].concat();
    scratch.append(&appended);
    let next = tail.poll().expect("second poll");

    assert_eq!(
        next.bytes_read,
        appended.len() as u64,
        "only the appended bytes may be read; reading {} of a {}-byte file means the reader \
         started over",
        next.bytes_read,
        first.len() + appended.len(),
    );
    assert_eq!(next.lines.len(), 2);
    assert!(!next.restarted);

    // The offsets are absolute positions in the file, not positions within the poll.
    assert_eq!(next.lines[0].0, first.len() as u64);

    // A poll with nothing new reads nothing at all.
    let idle = tail.poll().expect("third poll");
    assert_eq!(idle.bytes_read, 0);
    assert!(idle.lines.is_empty());
}

/// TC-40, the other half — a poll that lands mid-record holds the partial line back until
/// its newline arrives, and then reports it at the offset it really started on.
#[test]
fn tc_40_a_partial_line_is_buffered_until_its_newline() {
    let lines = recorded_lines();
    let scratch = Scratch::new("tc40-partial");

    scratch.append(&lines[0]);
    let mut tail = Tail::new(scratch.path());
    tail.poll().expect("first poll");

    let second = &lines[1];
    let split = second.len() / 2;
    scratch.append(&second[..split]);

    let mid = tail.poll().expect("poll mid-record");
    assert!(
        mid.lines.is_empty(),
        "half a record is not a record and must not be handed on: {:?}",
        mid.lines,
    );

    scratch.append(&second[split..]);
    let complete = tail.poll().expect("poll after the rest arrived");
    assert_eq!(complete.lines.len(), 1, "the record arrives once, whole");
    assert_eq!(
        complete.lines[0].0,
        lines[0].len() as u64,
        "it is reported at the offset it started at, not where the newline landed",
    );
    assert_eq!(complete.lines[0].1, second.trim_end_matches(['\n', '\r']));
}

/// TC-41 — truncation and replacement reset the offset.
///
/// Without this, the stored offset sits past the end of the new file and the reader either
/// sees nothing forever or reads from the middle of a record.
#[test]
fn tc_41_truncation_resets_the_offset() {
    let lines = recorded_lines();
    let scratch = Scratch::new("tc41");

    scratch.append(&lines[..4].concat());
    let mut tail = Tail::new(scratch.path());
    let before = tail.poll().expect("first poll");
    assert_eq!(before.lines.len(), 4);
    assert!(tail.offset() > 0);

    // The file is replaced by a shorter one — what a rotation or a fresh session looks like.
    let replacement: String = lines[..2].concat();
    scratch.write(&replacement);

    let after = tail.poll().expect("poll after truncation");
    assert!(after.restarted, "the shrink has to be noticed");
    assert_eq!(
        after.lines.len(),
        2,
        "the new file is read from the start rather than from a stale offset",
    );
    assert_eq!(after.lines[0].0, 0);
    assert_eq!(tail.offset(), replacement.len() as u64);
}

/// A transcript that has not been created yet is not an error: a session registers slightly
/// before its transcript exists, and FR-40 says a missing source is ignored, not reported.
#[test]
fn a_missing_transcript_is_not_an_error() {
    let scratch = Scratch::new("missing");
    let mut tail = Tail::new(scratch.path());

    let polled = tail.poll().expect("a missing file is not an I/O failure");
    assert!(polled.lines.is_empty());
    assert_eq!(polled.bytes_read, 0);
}

/// Resuming from a remembered offset reads only what arrived while the board was away.
#[test]
fn resuming_reads_only_what_is_new() {
    let lines = recorded_lines();
    let scratch = Scratch::new("resume");
    let seen: String = lines[..2].concat();
    scratch.append(&seen);

    let appended: String = lines[2..4].concat();
    scratch.append(&appended);

    let mut tail = Tail::resuming_at(scratch.path(), seen.len() as u64);
    let polled = tail.poll().expect("poll after resuming");

    assert_eq!(polled.bytes_read, appended.len() as u64);
    assert_eq!(polled.lines.len(), 2);
    assert_eq!(polled.lines[0].0, seen.len() as u64);
}

/// A line that is not valid UTF-8 is reported and skipped, never repaired.
///
/// Substituting replacement characters could turn a corrupt record into one that still
/// parses as JSON — a confident wrong reading rather than an honest gap (FR-39).
#[test]
fn an_undecodable_line_is_reported_not_repaired() {
    let lines = recorded_lines();
    let scratch = Scratch::new("utf8");
    scratch.append(&lines[0]);

    // A lone 0x80 continuation byte: JSON-shaped around it, invalid UTF-8 within it.
    let corrupt: Vec<u8> = [
        br#"{"type":"user","x":""#.as_slice(),
        &[0x80],
        br#""}"#.as_slice(),
        b"\n".as_slice(),
    ]
    .concat();
    fs::OpenOptions::new()
        .append(true)
        .open(scratch.path())
        .expect("open scratch file")
        .write_all(&corrupt)
        .expect("append corrupt bytes");
    scratch.append(&lines[1]);

    let mut tail = Tail::new(scratch.path());
    let polled = tail.poll().expect("poll");

    assert_eq!(
        polled.undecodable,
        vec![lines[0].len() as u64],
        "the undecodable line is reported, at the offset it started on",
    );
    assert_eq!(
        polled.lines.len(),
        2,
        "its neighbours are unaffected: a corrupt line costs exactly itself",
    );
    for (_, text) in &polled.lines {
        assert!(
            !text.contains('\u{fffd}'),
            "no line may come back with substituted bytes: {text}",
        );
    }
}
