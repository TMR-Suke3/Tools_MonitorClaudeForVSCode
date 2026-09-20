//! The board's view model: what the window draws, as data.
//!
//! Kept separate from the window so it can be built and checked without one. Everything the
//! front end needs is decided here — the roll-up status of a folded group, the fitted title
//! for a row, whether a workspace is drawn as a header plus rows or as a single merged row —
//! and the markup then does nothing but lay it out.
//!
//! `docs/ui-overlay.md` §2 is the geometry and §2.3 the merged-row rule; the roll-up
//! precedence is `docs/session-state-model.md` §8, and it lives in the core because it is a
//! statement about statuses rather than about pixels.

use mcv_core::machine::roll_up;
use mcv_core::palette::{Status, Theme, indicator, to_hex_of};
use mcv_core::title::{Fitted, Sources, fit_best};
use serde::Serialize;

/// Width of the title column when a group is open, in cells — the 300 px row less the
/// chrome around it (§2). Titles are fitted to this before they reach the front end, so the
/// markup never has to decide what to cut.
pub const TITLE_BUDGET_CELLS: usize = 30;

/// The whole board.
#[derive(Debug, Clone, Serialize)]
pub struct BoardView {
    pub theme: &'static str,
    /// How the board is being looked at. Not part of what the board *is* — a development
    /// affordance, so that a screenshot can be compared with another one.
    pub inspect: Inspect,
    /// The theme's own four colours (§3.2). Passed through rather than duplicated in the
    /// stylesheet, so that the palette has exactly one home.
    pub chrome: Chrome,
    pub groups: Vec<GroupView>,
    /// The status the **whole board** stands for — every session on it, rolled up once.
    ///
    /// Drawn nowhere on the board itself, and so not sent to the front end: it is what the
    /// tray icon carries while the board is hidden (FR-32). It is computed here rather than
    /// there so that the precedence of `session-state-model.md` §8 has one home — a tray that
    /// decided for itself which of two statuses was louder would be a second, quieter copy of
    /// that table, and the two would drift.
    #[serde(skip)]
    pub rollup: Status,
    /// True when no group is open, which is what lets the board narrow to its names (§2.5).
    pub fully_folded: bool,
    /// Whether the board is currently earning the word "exact" (FR-55).
    ///
    /// `"exact"`, `"went-quiet"` or `"inferred"` — the mode the *whole board* is in, because
    /// the hooks are installed for the machine rather than for a session. Shown rather than
    /// implied: a board that quietly stopped being exact would be presenting a confidence it
    /// is no longer earning, which is the one thing FR-52 forbids.
    pub mode: &'static str,
    /// How large the user asked for the board, as a percentage of the design's own size
    /// (§2.8). One of [`SCALES`]; the front end applies it, and every figure below is
    /// multiplied by it once, at the end.
    pub scale: u32,
    /// What the renderer measured the folded board to need, once it has.
    ///
    /// Absent until then, and absent whenever anything is open. The font is proportional and
    /// §2 is written in pixels, so the width of "the longest workspace name plus the chrome
    /// around it" is not a number this side can compute — the same reason the last fit of a
    /// title belongs to the browser.
    pub folded_width: Option<u32>,
}

/// Options that exist for looking at the board rather than for using it.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Inspect {
    /// Freeze every animation.
    ///
    /// Two reasons, and the second is the one that bites. A pulsing indicator photographed
    /// at an arbitrary moment is a different colour every time, so two screenshots of the
    /// same board differ for no reason — which once sent a review chasing a palette bug that
    /// was a blink caught mid-cycle. And this is also what the board looks like under the
    /// OS's reduced-motion setting, which §3.1 requires to lose no information (TC-54).
    pub still: bool,
    /// Draw everything at this multiple, for inspecting a 10 px silhouette.
    ///
    /// The design does the same thing: `indicator-states.svg` draws each indicator at 60 px
    /// beside its 10 px self, because a silhouette cannot be judged at the size it ships at.
    pub zoom: u32,
}

/// The board's own colours: everything drawn that is not an indicator.
#[derive(Debug, Clone, Serialize)]
pub struct Chrome {
    pub board: String,
    pub hairline: String,
    pub text: String,
    pub secondary: String,
    /// The board's outline and its title line (§2.6). Here rather than in the stylesheet for
    /// the same reason as the four above: the palette has exactly one home, and the light
    /// theme needs different values rather than the same ones dimmed.
    pub edge: String,
    pub titlebar: String,
    pub titlebar_hairline: String,
}

impl Chrome {
    /// The chrome of a theme.
    #[must_use]
    pub fn of(theme: Theme) -> Self {
        Self {
            board: to_hex_of(theme.board()),
            hairline: to_hex_of(theme.hairline()),
            text: to_hex_of(theme.text()),
            secondary: to_hex_of(theme.secondary()),
            edge: to_hex_of(theme.edge()),
            titlebar: to_hex_of(theme.titlebar()),
            titlebar_hairline: to_hex_of(theme.titlebar_hairline()),
        }
    }
}

impl StatusView {
    /// The same status, drawn as an outline.
    ///
    /// Used for a workspace's own mark while the workspace is open — the shape says *parent*,
    /// not *which status* (FR-17,
    /// [ADR-0024](../../../docs/adr/0024-colour-carries-the-status-shape-carries-the-hierarchy.md)).
    /// The colour, the name and the motion are untouched, so the mark still answers "what is
    /// in there".
    #[must_use]
    pub fn hollow(mut self) -> Self {
        self.silhouette = "hollow";
        self
    }
}

/// One workspace.
#[derive(Debug, Clone, Serialize)]
pub struct GroupView {
    /// The `cwd`, used as the identity. Never drawn.
    pub key: String,
    /// The folder's last segment — what the header shows (FR-04).
    pub label: String,
    pub folded: bool,
    /// A workspace with exactly one session is drawn as a single merged row: no header, no
    /// count, no chevron (§2.3). A header saying "1" above a row saying everything is pure
    /// duplication, and it doubles the height of the commonest kind of group.
    pub merged: bool,
    /// The status the group stands for while folded (§8).
    pub rollup: StatusView,
    pub sessions: Vec<SessionView>,
}

/// One session row.
#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub id: String,
    /// The title, already fitted to the column.
    pub title: String,
    /// The whole title, for the tooltip (FR-22).
    pub full_title: String,
    pub status: StatusView,
    /// The right-hand column: time in status, or a reset clock time for `limited` (FR-24).
    pub time: String,
}

/// A status, resolved to everything the markup needs so that the front end holds no palette
/// of its own. One palette, in one place, checked by TC-48 … TC-54.
#[derive(Debug, Clone, Serialize)]
pub struct StatusView {
    pub name: &'static str,
    pub colour: String,
    /// The silhouette, as a class name the stylesheet keys on (FR-17).
    pub silhouette: &'static str,
    /// The motion, likewise. Reduced motion is handled in CSS, not here.
    pub motion: &'static str,
}

impl StatusView {
    #[must_use]
    pub fn of(status: Status, theme: Theme) -> Self {
        let rendering = mcv_core::palette::rendering(status);
        Self {
            name: status.name(),
            colour: to_hex_of(indicator(status, theme)),
            silhouette: silhouette_class(rendering.silhouette),
            motion: motion_class(rendering.motion),
        }
    }
}

const fn silhouette_class(silhouette: mcv_core::palette::Silhouette) -> &'static str {
    use mcv_core::palette::Silhouette;
    match silhouette {
        Silhouette::FilledDisc => "filled",
        Silhouette::HollowRing => "hollow",
        Silhouette::DottedRing => "dotted",
    }
}

const fn motion_class(motion: mcv_core::palette::Motion) -> &'static str {
    use mcv_core::palette::Motion;
    match motion {
        Motion::None => "still",
        Motion::HaloExpand => "halo-expand",
        Motion::HaloStill => "halo-still",
        Motion::Blink => "blink",
    }
}

/// What one session contributes to the board, before it is dressed for display.
pub struct SessionState<'a> {
    pub id: &'a str,
    pub status: Status,
    pub ai_title: Option<&'a str>,
    /// The registry's name, only when somebody chose it (`nameSource` is not `derived`).
    pub registry_name: Option<&'a str>,
    /// The first line of the session's opening human prompt — what the editor's tab shows
    /// until Claude Code has generated a title (ADR-0030).
    pub prompt_line: Option<&'a str>,
    /// The registry's name when it *is* derived: a slug, and the last thing worth showing
    /// before falling back to the workspace label.
    pub derived_name: Option<&'a str>,
    pub time: String,
}

/// What one workspace contributes.
pub struct GroupState<'a> {
    pub key: &'a str,
    pub label: &'a str,
    pub folded: bool,
    pub sessions: Vec<SessionState<'a>>,
}

/// Height of one row, and the padding above and below the list, from §2. The board's height
/// follows its content — it is a list of rows, not a panel with space left over.
pub const ROW_HEIGHT: u32 = 25;
pub const PADDING: u32 = 5;
/// The title line above the list (§2). Always drawn: it is what says the window can be
/// dragged, and an affordance that appears once you already know where to point is not one.
pub const TITLE_HEIGHT: u32 = 28;
/// The border, top and bottom. Drawn *inside* the width, so only the height has to reserve
/// it — the window has no decorations and this outline is all it has.
pub const EDGE: u32 = 1;
/// Transparent space between the panel the user sees and the window that holds it (§2.7).
///
/// The window is larger than the board on every side, and the difference is see-through. It
/// is where the one thing that overflows the panel is drawn: the wave that leaves an
/// `awaiting_user` indicator
/// ([ADR-0022](../../../docs/adr/0022-the-window-is-larger-than-the-board.md)). The shadow
/// used to share it and no longer does
/// ([ADR-0034](../../../docs/adr/0034-the-board-casts-no-shadow.md)); the wave is what sets
/// the size.
pub const MARGIN: u32 = 16;
/// Width while any group is open (§2). A fully folded board narrows to its names instead.
pub const OPEN_WIDTH: u32 = 300;
/// The sizes the board can be drawn at, as percentages of the design's own (§2.8).
///
/// Steps rather than a slider, and the reason is arithmetic: every dimension in §2 is a whole
/// number of CSS pixels, and a 137 % board rounds each of them separately — a 1 px error in
/// the row height is 15 px down a full board. Four steps also means the folded width is
/// measured four times at most, rather than on every drag of a slider.
///
/// The same vocabulary Windows uses for display scaling, deliberately: a user who has met
/// 125 % once should not have to learn a second scale for one window.
pub const SCALES: [u32; 4] = [100, 125, 150, 200];

/// The narrowest the board may be, however short its workspace names (§2.5).
pub const MIN_WIDTH: u32 = 140;

impl BoardView {
    /// How many rows the board draws: one per group header, one per session in an open
    /// group, and one for each merged single-session workspace.
    #[must_use]
    pub fn row_count(&self) -> u32 {
        self.groups
            .iter()
            .map(|group| {
                // A merged workspace and a folded one are both one row; an open group is its
                // header plus its sessions.
                if group.merged || group.folded {
                    1
                } else {
                    1 + group.sessions.len() as u32
                }
            })
            .sum()
    }

    /// The height of the panel: the title line, the rows, the padding and the two edges.
    #[must_use]
    pub fn panel_height(&self) -> u32 {
        TITLE_HEIGHT + self.row_count() * ROW_HEIGHT + PADDING * 2 + EDGE * 2
    }

    /// The window height, which is the panel plus the transparent margin above and below
    /// (§2.7).
    #[must_use]
    pub fn height(&self) -> u32 {
        self.drawn(self.panel_height() + MARGIN * 2)
    }

    /// The width of the panel — the board as the user sees it.
    ///
    /// **Two discrete widths, never a continuous fit** (§2.5). A board that re-measured
    /// itself whenever a title changed would twitch in the corner of the eye all day, and
    /// titles change mid-session. So: 300 while anything is open, and while everything is
    /// folded, whatever the longest workspace name needs — which only the renderer can
    /// measure, and which it reports back through [`Self::folded_width`].
    #[must_use]
    pub fn panel_width(&self) -> u32 {
        let logical = if self.fully_folded {
            self.folded_width.unwrap_or(OPEN_WIDTH)
        } else {
            OPEN_WIDTH
        };
        logical.clamp(MIN_WIDTH, OPEN_WIDTH)
    }

    /// The window width, which is the panel plus the transparent margin on either side
    /// (§2.7). The clamp above is about the panel; the margin is added outside it rather than
    /// eating into it.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.drawn(self.panel_width() + MARGIN * 2)
    }

    /// A figure from §2, at the size the board is actually drawn.
    ///
    /// **Multiplied once, at the end.** Scaling each part and adding them up rounds four
    /// times and puts the error in the total; scaling the total rounds once. At 125 % a row
    /// is 31.25 px, which the browser draws happily — it is only the window that has to be a
    /// whole number of pixels.
    ///
    /// Two multipliers, kept apart on purpose. `inspect.zoom` is the development flag that
    /// blows the board up for a screenshot; `scale` is what the user chose from the menu. One
    /// number would make "why is this three times the size" unanswerable.
    ///
    /// **Rounded, not truncated.** Integer division floors, so a 147 px board at 125 % came
    /// out 183 rather than 184 — a window a fraction shorter than the picture inside it, and
    /// the last row clipped by a pixel. "Multiplied once, at the end" was true; "rounds once"
    /// was not, until this said `+ 50`.
    fn drawn(&self, logical: u32) -> u32 {
        (logical * self.scale + 50) / 100 * self.inspect.zoom.max(1)
    }

    /// Which workspaces the board is showing, as one string.
    ///
    /// The folded width is recomputed only when this changes (§2.5, ADR-0015): a board that
    /// re-measured itself whenever a title or a status moved would twitch in the corner of
    /// the eye all day. The renderer keys its measurement on the same thing.
    #[must_use]
    pub fn workspace_signature(&self) -> String {
        self.groups
            .iter()
            .map(|group| group.label.as_str())
            .collect::<Vec<_>>()
            .join("\u{0}")
    }

    /// Folds or unfolds one workspace, in place.
    ///
    /// Folding is a presentation change and nothing else — the same sessions, the same
    /// statuses, drawn differently — so it does not go back to the observer for a new board.
    /// That also keeps a click off the watcher's lock, which the polling thread holds while
    /// it reads files.
    pub fn set_folded(&mut self, key: &str, folded: bool) {
        for group in &mut self.groups {
            if group.key == key && !group.merged {
                group.folded = folded;
            }
        }
        // A board is folded when the user has folded it: every group that *can* fold is
        // folded, and at least one could. A one-session workspace has no fold state (§2.3).
        let mut any = false;
        let mut all = true;
        for group in self.groups.iter().filter(|g| !g.merged) {
            any = true;
            all &= group.folded;
        }
        self.fully_folded = any && all;
    }
}

/// Builds the view.
///
/// Groups keep the order they are given: first-seen order, never alphabetical, because
/// FR-23 requires the board's order to be stable across restarts and a sort here would make
/// that impossible to preserve.
#[must_use]
pub fn build(groups: &[GroupState<'_>], theme: Theme, inspect: Inspect) -> BoardView {
    // A board is folded when the user has folded it — every group that *can* fold is folded,
    // and at least one could. A one-session workspace has no fold state at all (§2.3), so a
    // board made only of merged rows is not a folded board: nothing has been folded, the
    // titles stay, and the width stays at 300.
    let foldable = groups.iter().filter(|g| g.sessions.len() > 1);
    let mut any_foldable = false;
    let mut all_folded = true;
    for group in foldable {
        any_foldable = true;
        all_folded &= group.folded;
    }
    let fully_folded = any_foldable && all_folded;

    // The board's own roll-up, over every session on it rather than over the groups' roll-ups.
    // The same answer either way — the precedence is a minimum — but rolling up the rolled-up
    // would make that a fact about `roll_up` that this line depends on without saying so.
    let overall = roll_up(
        groups
            .iter()
            .flat_map(|group| group.sessions.iter().map(|session| session.status)),
    )
    // An empty board is `unknown`, which is what §9 already draws for one: the marker that
    // says the tool is alive and is not claiming to know anything.
    .unwrap_or(Status::Unknown);

    BoardView {
        theme: match theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
        },
        inspect,
        // Inference until something says otherwise: the default mode, and not a degraded one.
        // The watcher replaces it with what the event log says (`watch::live`).
        mode: crate::events::Mode::Inferred.label(),
        // The design's own size until the board is told otherwise, which the app does from
        // the remembered settings on the first poll.
        scale: 100,
        folded_width: None,
        chrome: Chrome::of(theme),
        rollup: overall,
        fully_folded,
        groups: groups
            .iter()
            .filter(|group| !group.sessions.is_empty())
            .map(|group| {
                let rollup =
                    roll_up(group.sessions.iter().map(|s| s.status)).unwrap_or(Status::Unknown);
                GroupView {
                    key: group.key.to_owned(),
                    label: group.label.to_owned(),
                    folded: group.folded,
                    merged: group.sessions.len() == 1,
                    // An **open** workspace's own mark is drawn hollow, so that it and the
                    // session marks beneath it are not read as a row of equal things. A
                    // folded one stays solid: there the mark is the only evidence there is,
                    // and it has to carry the group on its own (FR-18, §3.1).
                    rollup: if group.folded || group.sessions.len() == 1 {
                        StatusView::of(rollup, theme)
                    } else {
                        StatusView::of(rollup, theme).hollow()
                    },
                    sessions: group
                        .sessions
                        .iter()
                        .map(|session| {
                            let Fitted { fitted, full, .. } = fit_best(
                                Sources {
                                    ai_title: session.ai_title,
                                    registry_name: session.registry_name,
                                    prompt_line: session.prompt_line,
                                    derived_name: session.derived_name,
                                    workspace_label: group.label,
                                },
                                TITLE_BUDGET_CELLS,
                            );
                            SessionView {
                                id: session.id.to_owned(),
                                title: fitted,
                                full_title: full,
                                status: StatusView::of(session.status, theme),
                                time: session.time.clone(),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}
