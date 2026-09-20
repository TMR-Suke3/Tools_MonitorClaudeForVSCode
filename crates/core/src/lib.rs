//! The OS-independent core of the board: observation, parsing, the status state machine,
//! title fitting — and the palette, which is data.
//!
//! This crate contains **no platform code and no UI code** (NFR-10) and must stay testable
//! headlessly from the recordings in `tests/fixtures/` (NFR-11). Windows-specific work
//! lives in the board crate, behind a thin module of its own
//! ([ADR-0004](../../../docs/adr/0004-use-tauri-and-rust.md)).
//!
//! The core is a **pure function of an observation stream**: it never reads the clock, the
//! filesystem or the process table, because everything it needs arrives as data
//! ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
//! That is what NFR-11's TC-45a checks, by taking all three away and running the suite.
//!
//! Implemented so far: the palette, transcript parsing, session discovery, the status state
//! machine, and title fitting. The hook layer's override path arrives with
//! `feat/hook-setup`, once its events have been observed rather than guessed at.

pub mod color;
pub mod discovery;
pub mod event;
pub mod machine;
pub mod palette;
pub mod record;
pub mod session;
pub mod time;
pub mod title;
