//! Following an append-only transcript by byte offset (NFR-02).
//!
//! This is the outer layer: it touches the filesystem, which the core is not allowed to do
//! ([ADR-0019](../../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
//! Its whole job is to turn a growing file into the (offset, line) pairs the core parses,
//! and to do it without ever re-reading what it has already seen — a transcript grows past
//! several hundred KB within one session, so reading from the start on every poll is the
//! one implementation mistake that would make the board expensive to run.
//!
//! Two things make that harder than "seek and read":
//!
//! * **A poll can land mid-line.** The file is being appended to as it is read, so the last
//!   bytes are often half a record. They are held back until their newline arrives rather
//!   than handed over as a broken line (TC-38's other half — the core rejects such a line,
//!   but it should never have to see one).
//! * **A file can shrink.** Truncation or replacement leaves the stored offset past the end,
//!   where seeking reads nothing or garbage. The offset resets and the file is re-read
//!   (TC-41).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// What one poll produced.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Polled {
    /// Complete lines, each with the byte offset it starts at. That offset is the ordering
    /// key the state machine uses
    /// ([ADR-0006](../../../../docs/adr/0006-order-events-by-append-position.md)).
    pub lines: Vec<(u64, String)>,
    /// Bytes actually read from the file this time. The measurement NFR-02 is about: a poll
    /// that read the whole file again would show it here.
    pub bytes_read: u64,
    /// Offsets of complete lines that were **not valid UTF-8**, and were skipped.
    ///
    /// Reported rather than repaired. Substituting replacement characters would produce a
    /// line that might still parse as JSON — a corrupt record quietly turned into a
    /// plausible one, which is worse than losing it and saying so (FR-39). A transcript is
    /// UTF-8, and a partial write is already held back until its newline, so a non-empty
    /// list here means real corruption rather than a poll that landed awkwardly.
    pub undecodable: Vec<u64>,
    /// The file shrank and was re-read from the start.
    pub restarted: bool,
}

/// A transcript being followed.
#[derive(Debug)]
pub struct Tail {
    path: PathBuf,
    /// Bytes consumed so far, including any held-back partial line.
    offset: u64,
    /// Bytes after the last newline: an incomplete record, waiting for the rest.
    partial: Vec<u8>,
    /// Where `partial` began, so a line completed by a later poll still reports the offset
    /// it actually started at.
    partial_offset: u64,
}

impl Tail {
    /// Follows `path` from its beginning.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            partial: Vec::new(),
            partial_offset: 0,
        }
    }

    /// Follows `path` from a remembered position — a session already being watched when the
    /// board restarts.
    #[must_use]
    pub fn resuming_at(path: impl Into<PathBuf>, offset: u64) -> Self {
        Self {
            path: path.into(),
            offset,
            partial: Vec::new(),
            partial_offset: offset,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How far into the file this tail has consumed.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Reads whatever has been appended since the last poll.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error. A transcript that has not appeared yet is **not**
    /// one: a missing file yields an empty poll, because a session's transcript is created
    /// slightly after the session registers and that race is normal (FR-40).
    pub fn poll(&mut self) -> io::Result<Polled> {
        let mut file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Polled::default()),
            Err(e) => return Err(e),
        };

        let len = file.metadata()?.len();
        let mut out = Polled::default();

        if len < self.offset {
            // Truncated or replaced. Anything held back belongs to a file that no longer
            // exists, so it goes with it.
            self.offset = 0;
            self.partial.clear();
            self.partial_offset = 0;
            out.restarted = true;
        }

        if len == self.offset {
            return Ok(out);
        }

        file.seek(SeekFrom::Start(self.offset))?;
        let mut fresh = Vec::with_capacity((len - self.offset) as usize);
        let read = file.read_to_end(&mut fresh)? as u64;
        out.bytes_read = read;

        let mut line_start = if self.partial.is_empty() {
            self.offset
        } else {
            self.partial_offset
        };
        let mut buffer = std::mem::take(&mut self.partial);
        buffer.extend_from_slice(&fresh);

        let mut consumed = 0usize;
        for chunk in buffer.split_inclusive(|&b| b == b'\n') {
            if chunk.last() != Some(&b'\n') {
                break; // The tail of the buffer is an incomplete line.
            }
            match std::str::from_utf8(chunk) {
                Ok(text) => {
                    let text = text.trim_end_matches(['\n', '\r']);
                    if !text.is_empty() {
                        out.lines.push((line_start, text.to_owned()));
                    }
                }
                Err(_) => out.undecodable.push(line_start),
            }
            line_start += chunk.len() as u64;
            consumed += chunk.len();
        }

        self.partial = buffer[consumed..].to_vec();
        self.partial_offset = line_start;
        self.offset += read;

        Ok(out)
    }
}
