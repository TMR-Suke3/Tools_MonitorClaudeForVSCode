//! Fitting a session title to the width it has (`docs/ui-overlay.md` §4).
//!
//! **There is deliberately no shortening logic.** The title arrives from Claude Code already
//! summarised, in whatever language the conversation uses, and the board shows it as it is
//! (FR-19). Summarising it again — dropping the filler, keeping the distinguishing part — is
//! a language-dependent judgement, and doing it without a model produces exactly the kind of
//! confident nonsense this product avoids elsewhere
//! ([ADR-0005](../../../docs/adr/0005-no-goal-achievement-status.md) is the same reasoning
//! applied to statuses).
//!
//! What is left is mechanical, and it is all this module does: normalise, then cut to fit.
//!
//! Two things make the cutting less obvious than it looks.
//!
//! * **Width is not length.** A CJK glyph occupies two cells and a Latin one occupies one,
//!   and 82 of the 98 measured titles contain East Asian characters — a title in Japanese is
//!   the normal case here, not the exception (FR-20).
//! * **A cut must land on a grapheme-cluster boundary.** Cutting inside one produces a
//!   broken glyph or a lone surrogate: the recordings carry a ZWJ emoji sequence, a combining
//!   acute and a variation selector precisely to catch that (FR-21).

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The character used to mark a title that did not fit.
pub const ELLIPSIS: &str = "…";

/// A title, fitted to a width, with the original kept alongside it.
///
/// Both halves are needed: the board draws [`Self::fitted`] and shows [`Self::full`] on
/// hover, so that truncation never destroys information (FR-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    /// What to draw. Never wider than the budget it was fitted to.
    pub fitted: String,
    /// The whole title, normalised but not cut.
    pub full: String,
    /// Whether anything was removed.
    pub truncated: bool,
}

/// Rendered width in terminal cells: 2 for East Asian Wide and Fullwidth, 1 for most else,
/// 0 for combining marks and joiners.
///
/// This is the width a renderer produces, which is **not** the naive per-code-point count.
/// The difference shows up exactly where the recordings put their derived rows: a ZWJ emoji
/// sequence is one glyph of 2 cells rather than three glyphs and two joiners.
#[must_use]
pub fn width(text: &str) -> usize {
    text.width()
}

/// Collapses whitespace and removes control characters (§4, step 1).
///
/// Titles are written by a model into a JSON string and can carry newlines, tabs and runs of
/// spaces. A board row is one line, so the normalising is not cosmetic: an un-normalised
/// title would either break the layout or be silently clipped by it.
#[must_use]
pub fn normalise(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_space = false;
    for grapheme in title.graphemes(true) {
        let is_space = grapheme.chars().all(char::is_whitespace);
        if is_space {
            // Any run of whitespace, of any kind, becomes one plain space — and a leading
            // run becomes nothing.
            pending_space = !out.is_empty();
            continue;
        }
        // Control characters are dropped rather than replaced: nothing in a title needs
        // them, and a replacement character would be a visible artefact of our own making.
        if grapheme.chars().all(|c| c.is_control()) {
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push_str(grapheme);
    }
    out
}

/// Fits a title into `budget` cells.
///
/// The result is never wider than the budget, is cut only on a grapheme-cluster boundary,
/// and ends in [`ELLIPSIS`] whenever anything was removed — with the ellipsis **inside** the
/// budget, not added past it (FR-20, FR-21).
///
/// A budget too small to hold even the ellipsis yields an empty string: there is no width in
/// which a mark of truncation would fit, and drawing one anyway would overflow the row.
#[must_use]
pub fn fit(title: &str, budget: usize) -> Fitted {
    let full = normalise(title);

    if width(&full) <= budget {
        return Fitted {
            fitted: full.clone(),
            full,
            truncated: false,
        };
    }

    let mark = width(ELLIPSIS);
    if budget < mark {
        return Fitted {
            fitted: String::new(),
            full,
            truncated: true,
        };
    }

    // Take clusters while what is kept plus the ellipsis still fits.
    let room = budget - mark;
    let mut fitted = String::new();
    let mut used = 0;
    for grapheme in full.graphemes(true) {
        let w = width(grapheme);
        if used + w > room {
            break;
        }
        fitted.push_str(grapheme);
        used += w;
    }
    fitted.push_str(ELLIPSIS);

    Fitted {
        fitted,
        full,
        truncated: true,
    }
}

/// Where a displayable title comes from, in order (TC-30).
///
/// Every source can be absent — a session that has not been titled yet has no `ai-title`
/// record, and the registry's name is only sometimes derived — so the chain ends at the
/// workspace folder, which always exists because it is the grouping key.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    /// The `ai-title` record's title. The most recent one wins.
    pub ai_title: Option<&'a str>,
    /// The registry's `name`, **only when somebody chose it** — that is, when `nameSource`
    /// is not `derived`. A chosen name outranks anything this product can work out.
    pub registry_name: Option<&'a str>,
    /// The first line of the session's opening human prompt. Used only here, and never
    /// stored (NFR-07).
    pub prompt_line: Option<&'a str>,
    /// The registry's `name` when it *is* derived — `notes-e8`, a slug made from the folder
    /// and the id. Below the prompt line because it is not a name anybody wrote, and above
    /// the workspace label because it at least tells two sessions of one workspace apart.
    pub derived_name: Option<&'a str>,
    /// The workspace folder's last segment — the label the group already shows.
    pub workspace_label: &'a str,
}

/// The best available title, fitted.
///
/// The chain always yields something: a row with no name at all would be a row the user
/// cannot identify, which is the one thing the board exists to prevent.
#[must_use]
pub fn fit_best(sources: Sources<'_>, budget: usize) -> Fitted {
    // The order is the editor's, not this product's: the tab shows the generated title once
    // there is one, and the opening prompt until then
    // ([ADR-0030](../../../docs/adr/0030-a-session-is-named-what-its-tab-is-named.md)). A
    // name somebody chose sits above both, and the slug below them.
    let chosen = [
        sources.ai_title,
        sources.registry_name,
        sources.prompt_line,
        sources.derived_name,
        Some(sources.workspace_label),
    ]
    .into_iter()
    .flatten()
    .find(|candidate| !normalise(candidate).is_empty())
    .unwrap_or(sources.workspace_label);

    fit(chosen, budget)
}
