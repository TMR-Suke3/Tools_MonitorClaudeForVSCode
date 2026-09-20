//! The board's own library half.
//!
//! The binary is a thin shell around this, and the split exists so the parts that touch the
//! filesystem and the process table can be tested at all: an integration test cannot reach
//! into a crate that is only a binary.

pub mod watch;

pub mod view;

pub mod demo;

pub mod settings;

/// Editing the user's `settings.json` — the one file this product writes that it does not
/// own. See the module for why it is shaped the way it is.
pub mod hooks;

/// Reading back what the hook helper wrote.
pub mod events;

/// The setup window that drives both of those, with the user's consent at every step.
pub mod setup;

/// The board's own menu, shared by the right-click and by the tray (FR-32).
pub mod menu;

/// The tray icon, which is where the board lives while it is hidden.
pub mod tray;

/// The two keys that work while something else has focus.
pub mod hotkeys;

pub mod app;

/// Where the board goes, and how it is found again when a display takes it away.
pub mod place;

pub mod raise;
pub mod win;
