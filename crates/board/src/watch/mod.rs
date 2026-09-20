//! The outer layer: everything the core is forbidden to do.
//!
//! Reading directories, following files by byte offset, and probing the process table all
//! live here, so that `mcv-core` stays a pure function of what it is handed
//! ([ADR-0019](../../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
//! Purity in the core does not make this code correct — it concentrates the part that can
//! only be tested against a real filesystem, which is what `tests/tail.rs` does.

pub mod tail;

pub mod process;

pub mod live;
