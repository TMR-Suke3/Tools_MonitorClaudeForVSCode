//! What the board remembers between runs (FR-28).
//!
//! Where the window was, which groups were folded, how big the user wants it drawn, and the
//! two keys they chose — and nothing else. **No session content is ever written**: not a
//! title, not a prompt, not a path beyond the workspace key a fold state belongs to
//! (NFR-07).
//!
//! The file lives in the platform's own config directory rather than beside the executable,
//! so a board run from a build tree and one run from an install share the same memory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The remembered state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Where the window's top-left corner was, in physical pixels.
    ///
    /// Checked against the displays that exist before it is used: a position remembered on a
    /// monitor that has since been unplugged would put the board somewhere nobody can reach
    /// it (TC-68), and a board you cannot see is worse than one in the wrong corner.
    #[serde(default)]
    pub position: Option<(i32, i32)>,

    /// Workspace keys whose groups are folded.
    ///
    /// A set of *folded* keys rather than a map of every group's state, so a workspace the
    /// board has never seen starts open — which is what a new group should do — and a
    /// workspace that disappears leaves nothing behind.
    #[serde(default)]
    pub folded: BTreeSet<String>,

    /// How large to draw the board, as a percentage. One of [`crate::view::SCALES`].
    ///
    /// Not the display's scaling — that is the platform's and the board already follows it
    /// (FR-29). This is the user saying the board should be bigger than the design's own
    /// size, which is a different question with a different answer per person and per desk.
    #[serde(default = "default_scale")]
    pub scale: u32,

    /// The two global shortcuts, as the plugin's own accelerator strings.
    #[serde(default)]
    pub hotkeys: Hotkeys,

    /// Whether a click also brings the session's own tab to the front.
    ///
    /// **Off unless the user turned it on**, and the default is the decision rather than a
    /// placeholder. Raising a window is something the board can verify afterwards; revealing
    /// a session is a URL handed to the editor, which decides for itself which of its windows
    /// receives it — and when it chooses wrong it opens that session there rather than doing
    /// nothing. That is the board rearranging an editor, which FR-33 otherwise forbids, so it
    /// happens only where somebody has asked for it
    /// ([ADR-0029](../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)).
    #[serde(default)]
    pub reveal_session: bool,
}

/// The keys the board listens for, whatever else has focus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkeys {
    /// Move the board to the display the mouse is on, raise it, and flash its edge once.
    ///
    /// The answer to the problem no border solves: on three screens, "which one is it on?"
    /// is a question about *finding* the board, and the board is the only thing that can
    /// answer it.
    #[serde(default = "default_summon")]
    pub summon: String,
    /// Hide the board, or bring it back where it was.
    #[serde(default = "default_toggle")]
    pub toggle: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            summon: default_summon(),
            toggle: default_toggle(),
        }
    }
}

fn default_scale() -> u32 {
    100
}

/// Chosen to be free rather than to be memorable: `Ctrl+Alt+…` is the range Windows itself
/// leaves alone, and a board that fails to register its own shortcut on first run would look
/// broken for a reason the user cannot see.
fn default_summon() -> String {
    "Ctrl+Alt+M".to_owned()
}

fn default_toggle() -> String {
    "Ctrl+Alt+B".to_owned()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: None,
            folded: BTreeSet::new(),
            scale: default_scale(),
            hotkeys: Hotkeys::default(),
            reveal_session: false,
        }
    }
}

impl Settings {
    /// The scale, forced into the set the board actually draws at.
    ///
    /// A file edited by hand, or written by an older build, can say anything; a board drawn
    /// at 137 % would be a board whose folded width was measured for a size it is not.
    #[must_use]
    pub fn scale(&self) -> u32 {
        if crate::view::SCALES.contains(&self.scale) {
            self.scale
        } else {
            default_scale()
        }
    }

    /// Reads the file, or returns the defaults.
    ///
    /// A missing file is the first run. A corrupt one is treated the same way: the board
    /// starts where it would have started anyway, which is a better answer than refusing to
    /// open (FR-39's spirit — degrade, do not stop).
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the file, creating its directory.
    ///
    /// # Errors
    ///
    /// Returns any I/O or encoding failure. Losing the remembered position is a nuisance
    /// rather than a fault, so callers log it and carry on.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }

    /// Whether a group should start folded.
    #[must_use]
    pub fn is_folded(&self, key: &str) -> bool {
        self.folded.contains(key)
    }

    /// Records a group's fold state. Returns whether anything changed.
    pub fn set_folded(&mut self, key: &str, folded: bool) -> bool {
        if folded {
            self.folded.insert(key.to_owned())
        } else {
            self.folded.remove(key)
        }
    }
}

/// A rectangle a window could be placed in, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Moves a remembered position onto a display that exists (TC-68).
///
/// Returns `None` when there is nothing to place it on, in which case the caller lets the
/// window open wherever the platform would have put it.
///
/// The rule is *nearest*, not *primary*: someone who kept the board on a second monitor and
/// unplugged it wants it back roughly where they had it, not thrown to the top-left of a
/// screen they were not using.
#[must_use]
pub fn clamp_to_displays(
    position: (i32, i32),
    size: (u32, u32),
    displays: &[Area],
) -> Option<(i32, i32)> {
    if displays.is_empty() {
        return None;
    }

    let (x, y) = position;
    let (width, height) = (size.0 as i32, size.1 as i32);

    // Already fully on a display? Leave it exactly where it was.
    let fits = |area: &Area| {
        x >= area.x
            && y >= area.y
            && x + width <= area.x + area.width as i32
            && y + height <= area.y + area.height as i32
    };
    if displays.iter().any(fits) {
        return Some(position);
    }

    // Otherwise put it on whichever display its corner is nearest to, pushed fully inside.
    // In i64 throughout. A virtual desktop can put a display far from the origin, and
    // squaring a large i32 difference overflows — which panics in a debug build and, worse,
    // silently picks the wrong display in a release one.
    let nearest = displays.iter().min_by_key(|area| {
        let cx = x.clamp(area.x, area.x + area.width as i32);
        let cy = y.clamp(area.y, area.y + area.height as i32);
        let (dx, dy) = (i64::from(x) - i64::from(cx), i64::from(y) - i64::from(cy));
        dx * dx + dy * dy
    })?;

    let max_x = nearest.x + (nearest.width as i32 - width).max(0);
    let max_y = nearest.y + (nearest.height as i32 - height).max(0);
    Some((x.clamp(nearest.x, max_x), y.clamp(nearest.y, max_y)))
}

/// Where the settings live.
#[must_use]
pub fn path_in(config_dir: &Path) -> PathBuf {
    config_dir.join("board.json")
}
