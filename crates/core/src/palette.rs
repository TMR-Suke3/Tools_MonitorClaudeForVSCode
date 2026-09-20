//! The palette and the indicator channels, as data.
//!
//! Every constant here is copied from [`docs/ui-overlay.md`] §3.1 – §3.3, which derived
//! them rather than picking them
//! ([ADR-0010](../../../docs/adr/0010-derive-the-palette-from-a-luminance-ladder.md)). That is
//! what lets FR-16 and FR-17 be checked with no window, no screenshot and no human:
//! `docs/test-plan.md` §7.1 turns the thresholds into TC-48 … TC-54, which live in
//! `tests/palette.rs`.
//!
//! When a test in that file fails, the thing that changes is the hex — not the threshold
//! (`docs/test-plan.md` §1, rule 2).
//!
//! [`docs/ui-overlay.md`]: ../../../docs/ui-overlay.md

use crate::color::{Rgb, contrast_ratio, rgb};

/// A session status, as the board draws it.
///
/// The five *displayed* statuses are the ones a session can be in;
/// [`Status::Unknown`] is the internal fallback the board draws when it has failed to work
/// something out, and is listed last whatever its lightness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    AwaitingUser,
    Working,
    Limited,
    Terminated,
    Idle,
    Unknown,
}

impl Status {
    /// All six, in the order `docs/ui-overlay.md` §3.2 tabulates them for the dark theme.
    pub const ALL: [Status; 6] = [
        Status::AwaitingUser,
        Status::Working,
        Status::Limited,
        Status::Terminated,
        Status::Idle,
        Status::Unknown,
    ];

    /// The five a session can actually be in. The dE2000 thresholds of §3.3 are stated
    /// over pairs of these; `Unknown` is held only to the contrast floor.
    pub const DISPLAYED: [Status; 5] = [
        Status::AwaitingUser,
        Status::Working,
        Status::Limited,
        Status::Terminated,
        Status::Idle,
    ];

    /// The name used in `docs/`, so an assertion failure names the row of the table.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Status::AwaitingUser => "awaiting_user",
            Status::Working => "working",
            Status::Limited => "limited",
            Status::Terminated => "terminated",
            Status::Idle => "idle",
            Status::Unknown => "unknown",
        }
    }
}

/// Which board background the indicators are drawn against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Dark, Theme::Light];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// The board background. Every indicator's contrast is measured against this and
    /// nothing else (`docs/ui-overlay.md` §3.3).
    #[must_use]
    pub const fn board(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#15181E"),
            Theme::Light => rgb("#F4F5F7"),
        }
    }

    /// The rule between rows.
    #[must_use]
    pub const fn hairline(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#2A2F38"),
            Theme::Light => rgb("#D4D8DE"),
        }
    }

    /// Session and workspace titles.
    #[must_use]
    pub const fn text(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#C9CFD8"),
            Theme::Light => rgb("#2C3138"),
        }
    }

    /// The time column, counts, and everything else subordinate to a title.
    #[must_use]
    pub const fn secondary(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#818A98"),
            Theme::Light => rgb("#6B727C"),
        }
    }

    /// The board's outline (`docs/ui-overlay.md` §2.6).
    ///
    /// The board is a borderless window sitting on top of an editor whose background is
    /// within a few percent of its own, and it was dissolving into it. The fill cannot fix
    /// that: `idle` sits at the 3:1 floor against [`Self::board`]
    /// ([ADR-0013](../../../docs/adr/0013-make-idle-the-faintest-thing-on-the-board.md)), so
    /// lightening the panel would drop the faintest status through it. The separation is all
    /// in this line — and it is a neutral, because every hue on this board already means
    /// something.
    #[must_use]
    pub const fn edge(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#55606F"),
            // Darker than the dark theme's edge is light. A light panel on a white editor is
            // nearly the same value as its background, so the border is doing the separating
            // on its own — where the dark theme has a lit top edge helping. Neither theme has
            // a shadow to lean on any more
            // ([ADR-0034](../../../docs/adr/0034-the-board-casts-no-shadow.md)).
            Theme::Light => rgb("#98A1AD"),
        }
    }

    /// The title line's fill, a step away from the board so the top of the window reads as a
    /// title bar — which is what tells the user it can be dragged (FR-27).
    ///
    /// Lighter than the board on the dark theme and darker on the light one: in both cases a
    /// step *towards* the text, which is the direction that reads as "this strip is chrome".
    ///
    /// **A wide step, not a hint.** The first version was one shade off the board and read as
    /// a seam rather than as a strip — the eye had to be told it was there. Both themes moved
    /// together so that the strip is the same distance from its board on each.
    #[must_use]
    pub const fn titlebar(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#2E3648"),
            Theme::Light => rgb("#DDE0E6"),
        }
    }

    /// The rule under the title line.
    #[must_use]
    pub const fn titlebar_hairline(self) -> Rgb {
        match self {
            Theme::Dark => rgb("#3B4557"),
            Theme::Light => rgb("#C4CAD3"),
        }
    }
}

/// The indicator colour for a status on a theme — the hexes of `docs/ui-overlay.md` §3.2,
/// verbatim.
#[must_use]
pub const fn indicator(status: Status, theme: Theme) -> Rgb {
    match (theme, status) {
        (Theme::Dark, Status::AwaitingUser) => rgb("#FDC405"),
        (Theme::Dark, Status::Working) => rgb("#08C5BD"),
        (Theme::Dark, Status::Limited) => rgb("#B759F7"),
        (Theme::Dark, Status::Terminated) => rgb("#EE0131"),
        (Theme::Dark, Status::Idle) => rgb("#676B6F"),
        (Theme::Dark, Status::Unknown) => rgb("#85898E"),
        (Theme::Light, Status::AwaitingUser) => rgb("#C96F00"),
        (Theme::Light, Status::Working) => rgb("#057381"),
        (Theme::Light, Status::Limited) => rgb("#7032CC"),
        (Theme::Light, Status::Terminated) => rgb("#9B001C"),
        (Theme::Light, Status::Idle) => rgb("#767A7E"),
        (Theme::Light, Status::Unknown) => rgb("#565A5E"),
    }
}

/// The shape of the mark, which is the channel that survives greyscale, colour-vision
/// deficiency and reduced motion (FR-17, `docs/ui-overlay.md` §3.1).
///
/// No two statuses may share one — that is TC-53, and it is asserted over this enum rather
/// than over pixels, because at 10 px the difference is not measurable any other way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Silhouette {
    /// Every status but one. The shape says nothing about *which* status this is — that is
    /// the palette's job, measured under greyscale and three colour-vision simulations
    /// ([ADR-0024](../../../docs/adr/0024-colour-carries-the-status-shape-carries-the-hierarchy.md)).
    FilledDisc,
    /// `unknown`, and only `unknown`: the one status that keeps an outline of its own.
    ///
    /// Not an exception made for looks. `unknown` means the observer cannot read the
    /// session, and §3.2 requires it to carry the least ink of the six — a fallback must not
    /// read as an alarm. Filled, at 5.05:1, it would be the third loudest mark on the board.
    DottedRing,
    /// **Not a status.** A workspace's mark, while that workspace is open and its sessions
    /// are drawn beneath it: hollow, so the parent and its children can be told apart at a
    /// glance. A folded workspace's mark is filled, because there it is the only mark there
    /// is (`docs/ui-overlay.md` §3.1).
    HollowRing,
}

/// What the mark does over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    /// Static. `working`, `idle` and `unknown` — and `working` is the interesting one:
    /// nothing about it moves, because the board must not move for a session that needs
    /// nothing ([ADR-0021](../../../docs/adr/0021-working-does-not-animate.md)).
    None,
    /// `awaiting_user`, ~0.9 s: the halo expands and fades.
    HaloExpand,
    /// `terminated`, ~1.2 s for the first 6 s, then static.
    Blink,
    /// `awaiting_user` under reduced motion: the wave, drawn where it starts and not
    /// travelling.
    ///
    /// FR-17 requires every status distinguished by motion to have a static substitute, and
    /// this is `awaiting_user`'s. It is a shape, which is the one place the amended FR-17's
    /// "the shape must not encode status" gives way — with the motion turned off there is
    /// nothing else for the substitute to be, and a summons that becomes an ordinary disc is
    /// the one loss the requirement exists to prevent.
    HaloStill,
}

impl Motion {
    /// Whether the mark moves on its own — which is what an OS reduced-motion setting
    /// asks to be spared, and so what needs a substitute in §3.1's table.
    ///
    /// Not the same as *continuous*: `Blink` stops after six seconds and still counts, while
    /// a ring that is simply drawn does not.
    #[must_use]
    pub const fn is_animated(self) -> bool {
        !matches!(self, Motion::None | Motion::HaloStill)
    }
}

/// How a status is drawn: the two non-colour channels together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rendering {
    pub silhouette: Silhouette,
    pub motion: Motion,
}

/// The normal rendering of a status (`docs/ui-overlay.md` §3.1).
#[must_use]
pub const fn rendering(status: Status) -> Rendering {
    let (silhouette, motion) = match status {
        Status::Working => (Silhouette::FilledDisc, Motion::None),
        Status::AwaitingUser => (Silhouette::FilledDisc, Motion::HaloExpand),
        Status::Idle => (Silhouette::FilledDisc, Motion::None),
        Status::Terminated => (Silhouette::FilledDisc, Motion::Blink),
        // No motion. The level that used to drain went with the silhouettes (FR-17), and a
        // usage limit does not ask the developer to move — the row shows the reset time as
        // text, which is what the countdown was for (§2.4).
        Status::Limited => (Silhouette::FilledDisc, Motion::None),
        Status::Unknown => (Silhouette::DottedRing, Motion::None),
    };
    Rendering { silhouette, motion }
}

/// The rendering used when the OS asks for reduced motion — the "substitute" column of
/// `docs/ui-overlay.md` §3.1.
///
/// The silhouette never changes. With the shapes no longer encoding status (FR-17), what
/// keeps the six apart under reduced motion is the palette, measured by TC-49 … TC-51.
#[must_use]
pub const fn reduced_motion_rendering(status: Status) -> Rendering {
    let motion = match status {
        // The wave stops travelling but is still drawn, sitting just off the disc. Dropping
        // it altogether would leave the summons as a plain amber disc — the same mark as
        // every other status, separated by colour alone, which is exactly what FR-17's
        // "static substitute" clause exists to prevent.
        Status::AwaitingUser => Motion::HaloStill,
        // `terminated` keeps its colour and its row; `working` never animated in the first
        // place (ADR-0021); nothing else moves at all.
        _ => Motion::None,
    };
    Rendering {
        silhouette: rendering(status).silhouette,
        motion,
    }
}

/// Relative area of the indicator's silhouette, in px² **on a nominal 10 px cell**.
///
/// A shape model rather than a measurement of the drawn mark: the board draws at 15 px
/// (`docs/ui-overlay.md` §2), which multiplies every figure below by 2.25 and changes no
/// comparison, because [`ink_index`] is only ever read as an ordering. Keeping the nominal
/// cell means these numbers stay comparable with the ones §3.2's table was computed from.
///
/// Two are modelled rather than measured, and it is worth knowing which:
///
/// * `unknown`'s dotted ring is modelled with a 1.2 px stroke, though §3.1 describes it as
///   1 px. It is the smallest of the six either way, so nothing asserted here turns on it.
/// * `limited`'s level is modelled half full, because it drains: its real area moves between
///   the ring alone and the ring plus the whole interior.
#[must_use]
pub fn indicator_area(status: Status) -> f64 {
    // Every mark is drawn on a circle of the cell's diameter
    // ([ADR-0023](../../../docs/adr/0023-every-mark-is-a-disc.md)), so these are circle
    // figures: a filled disc is πr², and a ring is its circumference times its stroke.
    const R: f64 = 5.0;
    let disc = std::f64::consts::PI * R * R;
    let ring = |mean_radius: f64, stroke: f64| 2.0 * std::f64::consts::PI * mean_radius * stroke;

    match status {
        // Five of the six are the same solid disc. What separates them is colour, and the
        // separation is measured rather than asserted (TC-49 … TC-51).
        Status::Working | Status::Idle | Status::Terminated | Status::Limited => disc,
        // The disc, plus the wave drawn where it starts: a 1.5 px ring at 1.25x the radius.
        // It is the one mark that carries something extra, and the extra is motion — the
        // status that means *go here* is allowed to move (ADR-0021).
        Status::AwaitingUser => disc + ring(R * 1.25, 1.5),
        // A dotted ring: 1.2 px stroke, 55 % of the circumference inked. The only outline
        // left, and the reason is ink rather than identification — see [`Silhouette`].
        Status::Unknown => ring(R - 0.6, 1.2) * 0.55,
    }
}

/// Relative ink index: drawn area × contrast against the indicator's own board.
///
/// "Prominence is contrast × area, not contrast alone" (`docs/ui-overlay.md` §3.2). The
/// absolute value means nothing; only the ordering does, and only the two ends of that
/// ordering are an invariant
/// ([ADR-0014](../../../docs/adr/0014-the-ladder-pins-the-ends-not-the-middle.md)).
#[must_use]
pub fn ink_index(status: Status, theme: Theme) -> f64 {
    indicator_area(status) * contrast_ratio(indicator(status, theme), theme.board())
}

/// What a confusion between two statuses costs, which is what sets their separation
/// threshold (`docs/ui-overlay.md` §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// A confusion sends the user to the wrong place, or stops them going at all.
    A,
    /// A confusion costs a glance: both members mean "nothing is demanded of you".
    B,
}

impl Tier {
    /// The minimum dE2000 for this tier, in every non-greyscale view.
    #[must_use]
    pub const fn minimum(self) -> f64 {
        match self {
            Tier::A => 15.0,
            Tier::B => 8.0,
        }
    }
}

/// The tier a pair of displayed statuses belongs to: everything involving
/// `awaiting_user`, plus `terminated` against `working` and against `idle`, is tier A.
#[must_use]
pub fn tier(a: Status, b: Status) -> Tier {
    let pair = |x, y| (a == x && b == y) || (a == y && b == x);
    if a == Status::AwaitingUser
        || b == Status::AwaitingUser
        || pair(Status::Terminated, Status::Working)
        || pair(Status::Terminated, Status::Idle)
    {
        Tier::A
    } else {
        Tier::B
    }
}

/// The minimum dE2000 for a pair **in greyscale**.
///
/// `awaiting_user` / `terminated` is the only pair sharing both a solid fill and a blink,
/// so in greyscale it is the only pair with nothing but colour left to separate it. Every
/// other pair differs in silhouette, and the silhouette does the work.
#[must_use]
pub fn greyscale_minimum(a: Status, b: Status) -> f64 {
    let critical = (a == Status::AwaitingUser && b == Status::Terminated)
        || (a == Status::Terminated && b == Status::AwaitingUser);
    if critical { 15.0 } else { 4.0 }
}

/// The `#RRGGBB` text of a palette colour, for a stylesheet or a template.
///
/// The palette is data, and this is the one conversion the presentation layer needs — so it
/// lives here rather than being re-derived wherever a colour is written out.
#[must_use]
pub fn to_hex_of(colour: Rgb) -> String {
    crate::color::to_hex(colour)
}

/// Every indicator must clear this against its own board background, including
/// `unknown` (FR-17, `docs/ui-overlay.md` §3.3 and §8).
pub const CONTRAST_FLOOR: f64 = 3.0;
