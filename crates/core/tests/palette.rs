//! TC-48 … TC-54 — the palette, checked as data.
//!
//! `docs/test-plan.md` §7.1: FR-16 and FR-17 look inherently visual, but the palette was
//! *derived* rather than picked, so both are checkable from the constants alone — no
//! window, no screenshot, no human. Each test below names the case and the requirement it
//! defends (`docs/test-plan.md` §1, rule 3).
//!
//! **When one of these fails, the hex changes — not the threshold** (rule 2). Changing an
//! expectation means changing the requirement it comes from, in its own commit.
//!
//! Several tests also assert the *published* numbers of `docs/ui-overlay.md` §3.2 and §3.3,
//! which are listed there as measured values. That makes the first run of this suite a
//! check that the document is telling the truth.

use mcv_core::color::{Vision, contrast_ratio, difference, lab};
use mcv_core::palette::{
    CONTRAST_FLOOR, Motion, Silhouette, Status, Theme, Tier, greyscale_minimum, indicator,
    indicator_area, ink_index, reduced_motion_rendering, rendering, tier,
};

// ------------------------------------------------ the thresholds, copied from the document
//
// These are written out as literals **on purpose**, and every assertion below compares
// against them rather than against `mcv_core`'s own constants. Taking the numbers from the
// crate under test would make the suite assert "the palette satisfies whatever the code
// currently demands", which passes just as happily after someone lowers a threshold — the
// exact move rule 2 of `docs/test-plan.md` §1 exists to stop. The crate's constants are
// themselves checked against these, once, in `tier_table_matches_the_document`.
//
// Source: `docs/ui-overlay.md` §3.3.

/// Minimum contrast of any indicator against its own board background.
const FLOOR: f64 = 3.0;
/// Tier A — a confusion sends the user to the wrong place, or stops them going at all.
const TIER_A_MIN: f64 = 15.0;
/// Tier B — a confusion costs a glance.
const TIER_B_MIN: f64 = 8.0;
/// Greyscale, for the one pair sharing both a solid fill and a blink.
const GREYSCALE_CRITICAL_MIN: f64 = 15.0;
/// Greyscale, every other pair — the silhouette carries the rest.
const GREYSCALE_MIN: f64 = 4.0;

/// Every pair §3.3 places in tier A: all four involving `awaiting_user`, plus `terminated`
/// against `working` and against `idle`.
const TIER_A_PAIRS: [(Status, Status); 6] = [
    (Status::AwaitingUser, Status::Working),
    (Status::AwaitingUser, Status::Limited),
    (Status::AwaitingUser, Status::Terminated),
    (Status::AwaitingUser, Status::Idle),
    (Status::Terminated, Status::Working),
    (Status::Terminated, Status::Idle),
];

/// The only greyscale pair held to tier A.
const GREYSCALE_CRITICAL_PAIR: (Status, Status) = (Status::AwaitingUser, Status::Terminated);

fn is_pair(a: Status, b: Status, pair: (Status, Status)) -> bool {
    (a, b) == pair || (b, a) == pair
}

/// Tier of a pair **according to the document**, not according to the crate.
fn documented_tier(a: Status, b: Status) -> Tier {
    if TIER_A_PAIRS.iter().any(|&p| is_pair(a, b, p)) {
        Tier::A
    } else {
        Tier::B
    }
}

/// Every unordered pair of displayed statuses.
fn displayed_pairs() -> Vec<(Status, Status)> {
    let all = Status::DISPLAYED;
    let mut out = Vec::new();
    for (i, &a) in all.iter().enumerate() {
        for &b in &all[i + 1..] {
            out.push((a, b));
        }
    }
    out
}

fn round_to(v: f64, places: u32) -> f64 {
    let f = 10f64.powi(places as i32);
    (v * f).round() / f
}

fn contrast_against_board(status: Status, theme: Theme) -> f64 {
    contrast_ratio(indicator(status, theme), theme.board())
}

// ---------------------------------------------------------------- TC-48, FR-16

/// TC-48 — every indicator clears 3:1 against its own board background.
///
/// Both themes, all six statuses including `unknown`. The tightest is `idle` on the dark
/// theme, which sits on the floor by design
/// ([ADR-0013](../../../docs/adr/0013-make-idle-the-faintest-thing-on-the-board.md)) — so
/// this is the assertion that catches a board background being darkened.
#[test]
fn tc_48_every_indicator_clears_three_to_one() {
    let mut tightest = (f64::MAX, Status::Unknown, Theme::Dark);
    for theme in Theme::ALL {
        for status in Status::ALL {
            let ratio = contrast_against_board(status, theme);
            assert!(
                ratio >= FLOOR,
                "{} on the {} board is {ratio:.2}:1, under the {FLOOR}:1 floor",
                status.name(),
                theme.name(),
            );
            if ratio < tightest.0 {
                tightest = (ratio, status, theme);
            }
        }
    }

    let (ratio, status, theme) = tightest;
    assert_eq!(
        (status, theme),
        (Status::Idle, Theme::Dark),
        "the tightest indicator is documented as `idle` on the dark board, but it is {} on \
         the {} board at {ratio:.2}:1",
        status.name(),
        theme.name(),
    );
    assert_eq!(
        round_to(ratio, 2),
        3.31,
        "ui-overlay.md §3.2 publishes 3.31:1 for `idle` on the dark board",
    );
}

/// TC-48, supporting — the contrast column of `docs/ui-overlay.md` §3.2 is correct.
#[test]
fn tc_48_published_contrast_column_is_correct() {
    let published = [
        (Theme::Dark, Status::AwaitingUser, 11.07),
        (Theme::Dark, Status::Working, 8.24),
        (Theme::Dark, Status::Limited, 4.90),
        (Theme::Dark, Status::Terminated, 3.97),
        (Theme::Dark, Status::Idle, 3.31),
        (Theme::Dark, Status::Unknown, 5.05),
        (Theme::Light, Status::Terminated, 7.99),
        (Theme::Light, Status::Limited, 6.38),
        (Theme::Light, Status::Working, 5.10),
        (Theme::Light, Status::Idle, 3.96),
        (Theme::Light, Status::AwaitingUser, 3.34),
        (Theme::Light, Status::Unknown, 6.37),
    ];
    for (theme, status, expected) in published {
        let actual = round_to(contrast_against_board(status, theme), 2);
        assert_eq!(
            actual,
            expected,
            "{} on the {} board: §3.2 says {expected:.2}:1, computed {actual:.2}:1",
            status.name(),
            theme.name(),
        );
    }
}

// ---------------------------------------------------------------- TC-49 / TC-50, FR-16

/// TC-49 — tier-A pairs stay 15 dE2000 apart in every view.
///
/// Normal vision plus protanopia, deuteranopia and tritanopia (Machado 2009, severity 1.0),
/// both themes. Tier A is every pair involving `awaiting_user`, plus `terminated` against
/// `working` and against `idle`: confusing any of these sends the user to the wrong place,
/// or stops them going at all.
#[test]
fn tc_49_tier_a_pairs_stay_fifteen_apart() {
    assert_pairs_meet_their_tier(Tier::A, TIER_A_MIN);
}

/// TC-50 — tier-B pairs stay 8 dE2000 apart in every view, both themes.
///
/// Both members of every one of these pairs mean "nothing is demanded of you", so a
/// confusion costs a glance; silhouette and motion carry the rest (FR-17).
#[test]
fn tc_50_tier_b_pairs_stay_eight_apart() {
    assert_pairs_meet_their_tier(Tier::B, TIER_B_MIN);
}

fn assert_pairs_meet_their_tier(want: Tier, minimum: f64) {
    let mut checked = 0;
    for theme in Theme::ALL {
        for (a, b) in displayed_pairs() {
            if documented_tier(a, b) != want {
                continue;
            }
            for view in Vision::COLOUR {
                let d = difference(indicator(a, theme), indicator(b, theme), view);
                assert!(
                    d >= minimum,
                    "{}/{} on the {} board under {} vision: dE2000 {d:.2}, tier {want:?} \
                     requires {minimum:.0}",
                    a.name(),
                    b.name(),
                    theme.name(),
                    view.name(),
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no pair fell into tier {want:?}");
}

/// TC-49 / TC-50, supporting — the worst-pair table of `docs/ui-overlay.md` §3.3 is
/// correct: the pair it names really is the tightest in that view, at the value it prints.
#[test]
fn tc_49_50_published_worst_pairs_are_correct() {
    // Short bindings so each published row stays on one line, the way §3.3 prints them.
    let (dark, light) = (Theme::Dark, Theme::Light);
    let (idle, limited) = (Status::Idle, Status::Limited);
    let (working, terminated) = (Status::Working, Status::Terminated);

    let published = [
        (dark, Vision::Normal, idle, limited, 23.2),
        (dark, Vision::Protanopia, idle, terminated, 20.5),
        (dark, Vision::Deuteranopia, working, limited, 20.9),
        (dark, Vision::Tritanopia, idle, limited, 23.4),
        (dark, Vision::Greyscale, idle, terminated, 5.1),
        (light, Vision::Normal, working, idle, 19.2),
        (light, Vision::Protanopia, working, idle, 8.8),
        (light, Vision::Deuteranopia, working, idle, 13.2),
        (light, Vision::Tritanopia, idle, limited, 18.8),
        (light, Vision::Greyscale, terminated, limited, 4.9),
    ];

    for (theme, view, want_a, want_b, want_d) in published {
        let (d, (a, b)) = displayed_pairs()
            .into_iter()
            .map(|(a, b)| {
                (
                    difference(indicator(a, theme), indicator(b, theme), view),
                    (a, b),
                )
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .expect("there is at least one pair");

        // The pair is unordered: §3.3 prints whichever way round reads better.
        let named = |mut p: [&str; 2]| {
            p.sort_unstable();
            p.join("/")
        };
        assert_eq!(
            named([a.name(), b.name()]),
            named([want_a.name(), want_b.name()]),
            "{} board, {} vision: §3.3 names a different worst pair (computed dE {d:.2})",
            theme.name(),
            view.name(),
        );
        assert_eq!(
            round_to(d, 1),
            want_d,
            "{} board, {} vision: §3.3 prints {want_d:.1}, computed {d:.2}",
            theme.name(),
            view.name(),
        );
    }
}

/// TC-49 / TC-50, supporting — `mcv_core`'s own thresholds and tier table say what
/// `docs/ui-overlay.md` §3.3 says.
///
/// Every other assertion in this file compares against the literals at the top, so that
/// lowering a threshold in the crate cannot make the suite easier to pass. This is the one
/// place the crate's constants are checked, and it is what turns such a change into a
/// failure instead of a silent weakening.
#[test]
fn tier_table_matches_the_document() {
    assert_eq!(
        CONTRAST_FLOOR, FLOOR,
        "§3.3 sets the contrast floor at {FLOOR}:1"
    );
    assert_eq!(
        Tier::A.minimum(),
        TIER_A_MIN,
        "§3.3 sets tier A at {TIER_A_MIN} dE2000"
    );
    assert_eq!(
        Tier::B.minimum(),
        TIER_B_MIN,
        "§3.3 sets tier B at {TIER_B_MIN} dE2000"
    );

    for (a, b) in displayed_pairs() {
        assert_eq!(
            tier(a, b),
            documented_tier(a, b),
            "{}/{} is in the wrong tier: §3.3 puts every pair involving `awaiting_user`, plus terminated/working and terminated/idle, in tier A",
            a.name(),
            b.name(),
        );

        let expected = if is_pair(a, b, GREYSCALE_CRITICAL_PAIR) {
            GREYSCALE_CRITICAL_MIN
        } else {
            GREYSCALE_MIN
        };
        assert_eq!(
            greyscale_minimum(a, b),
            expected,
            "{}/{} has the wrong greyscale threshold; only awaiting_user/terminated is held to {GREYSCALE_CRITICAL_MIN}",
            a.name(),
            b.name(),
        );
    }
}

// ---------------------------------------------------------------- TC-51, FR-16

/// TC-51 — greyscale separation.
///
/// `awaiting_user` vs `terminated` ≥ 15 dE2000 in greyscale: it is the only pair sharing
/// both a solid fill and a blink, so it is the only pair with nothing but colour left to
/// separate it. Every other pair ≥ 4, with the silhouette carrying the rest.
#[test]
fn tc_51_greyscale_separation() {
    for theme in Theme::ALL {
        for (a, b) in displayed_pairs() {
            let minimum = if is_pair(a, b, GREYSCALE_CRITICAL_PAIR) {
                GREYSCALE_CRITICAL_MIN
            } else {
                GREYSCALE_MIN
            };
            let d = difference(indicator(a, theme), indicator(b, theme), Vision::Greyscale);
            assert!(
                d >= minimum,
                "{}/{} on the {} board in greyscale: dE2000 {d:.2}, requires {minimum:.0}",
                a.name(),
                b.name(),
                theme.name(),
            );
        }
    }
}

// ---------------------------------------------------------------- TC-52, FR-16

/// TC-52 — the lightness ladder holds, on the dark theme.
///
/// A regression guard on the shape of the palette, and the constraint a future "nicer"
/// colour is most likely to break silently. It is **not** a claim that prominence follows
/// urgency throughout — that part is TC-52c
/// ([ADR-0014](../../../docs/adr/0014-the-ladder-pins-the-ends-not-the-middle.md)).
#[test]
fn tc_52_the_lightness_ladder_holds() {
    let rungs = [
        (Status::AwaitingUser, Status::Working, 10.0),
        (Status::Working, Status::Limited, 8.0),
        (Status::Limited, Status::Terminated, 4.0),
        (Status::Terminated, Status::Idle, 3.0),
    ];
    for (upper, lower, gap) in rungs {
        let (hi, lo) = (
            lab(indicator(upper, Theme::Dark))[0],
            lab(indicator(lower, Theme::Dark))[0],
        );
        assert!(
            hi - lo >= gap,
            "{} is {:.2} L* above {}, the ladder requires {gap:.0}",
            upper.name(),
            hi - lo,
            lower.name(),
        );
    }
}

/// TC-52, supporting — the L\* column of `docs/ui-overlay.md` §3.2 is correct.
#[test]
fn tc_52_published_lightness_column_is_correct() {
    let published = [
        (Theme::Dark, Status::AwaitingUser, 82),
        (Theme::Dark, Status::Working, 72),
        (Theme::Dark, Status::Limited, 56),
        (Theme::Dark, Status::Terminated, 50),
        (Theme::Dark, Status::Idle, 45),
        (Theme::Dark, Status::Unknown, 57),
        (Theme::Light, Status::Terminated, 32),
        (Theme::Light, Status::Limited, 38),
        (Theme::Light, Status::Working, 44),
        (Theme::Light, Status::Idle, 51),
        (Theme::Light, Status::AwaitingUser, 56),
        (Theme::Light, Status::Unknown, 38),
    ];
    for (theme, status, expected) in published {
        let actual = lab(indicator(status, theme))[0];
        assert_eq!(
            actual.round() as i32,
            expected,
            "{} on the {} board: §3.2 says L* {expected}, computed {actual:.2}",
            status.name(),
            theme.name(),
        );
    }
}

/// TC-52a — `idle` is the least prominent of the five displayed statuses, both themes.
///
/// Colour alone does not settle it, because the silhouettes differ in area: prominence is
/// the ink index, indicator area × contrast against its own board.
#[test]
fn tc_52a_idle_is_the_least_prominent_of_the_five() {
    for theme in Theme::ALL {
        let idle = ink_index(Status::Idle, theme);
        for status in Status::DISPLAYED {
            if status == Status::Idle {
                continue;
            }
            let other = ink_index(status, theme);
            assert!(
                idle < other,
                "on the {} board `idle` carries ink {idle:.0} and {} carries {other:.0}; \
                 the resting state must be the faintest of the five",
                theme.name(),
                status.name(),
            );
        }
    }
}

/// TC-52b — nothing renders below the contrast floor, in any state the board can put it in.
///
/// There is exactly one such state, because **there is no opacity ramp**: aging by
/// brightness was retired
/// ([ADR-0013](../../../docs/adr/0013-make-idle-the-faintest-thing-on-the-board.md)), and
/// this test shows why it cannot come back.
///
/// The alpha at which `idle` crosses the floor depends on how the compositing is done, and
/// the two models disagree: **86.6 %** in linear light — the figure ADR-0013 quotes as
/// "87 %" — but **92.2 %** in 8-bit sRGB, which is what a WebView does for CSS `opacity` and
/// is therefore the published one
/// ([ADR-0017](../../../docs/adr/0017-judge-opacity-by-srgb-compositing.md)). The assertions
/// below hold under both, so the case does not rest on that choice being right.
#[test]
fn tc_52b_nothing_renders_below_the_contrast_floor() {
    for theme in Theme::ALL {
        for status in Status::ALL {
            assert!(
                contrast_against_board(status, theme) >= FLOOR,
                "{} on the {} board is under the floor at full opacity",
                status.name(),
                theme.name(),
            );
        }
    }

    let idle = indicator(Status::Idle, Theme::Dark);
    let board = Theme::Dark.board();

    // The ramp that was retired: 55 % opacity after 30 minutes. Far under the floor either
    // way it is composited, so no gentler setting of the same idea survives either.
    //
    // `boundary` is the lowest whole-percent opacity at which `idle` still clears the floor,
    // per model. Pinning it exactly is what says an aging ramp has nowhere to go: even under
    // the more forgiving model there are seven points of room below full opacity, and the
    // published figure is the sRGB one (ADR-0017).
    for (model, composite, boundary) in [
        (
            "sRGB",
            mcv_core::color::composite_srgb as fn(_, _, _) -> _,
            93,
        ),
        ("linear", mcv_core::color::composite_linear, 87),
    ] {
        let aged = contrast_ratio(composite(idle, board, 0.55), board);
        assert!(
            aged < FLOOR,
            "the retired 55 % ramp gives {aged:.2}:1 under {model} compositing, which \
             would have been legal",
        );

        // Contrast rises monotonically with alpha, so the first percent that clears is the
        // boundary. Searching upwards matters: from the top, 100 % clears and the search
        // ends there having proved nothing.
        let lowest = (0..=100)
            .find(|&pct| {
                contrast_ratio(composite(idle, board, f64::from(pct) / 100.0), board) >= FLOOR
            })
            .expect("full opacity clears the floor");
        assert_eq!(
            lowest, boundary,
            "under {model} compositing `idle` clears the floor down to {lowest} % opacity, \
             not the documented {boundary} %",
        );
    }
}

/// TC-52c — the loud end of the ladder holds.
///
/// Two assertions with different scopes, because the ends of the ladder do not both survive
/// the light theme
/// ([ADR-0018](../../../docs/adr/0018-the-loud-end-is-a-dark-theme-property.md)):
///
/// * **Dark board** — `awaiting_user` carries the highest ink index of all six. On paper it
///   sits third and is not required to lead: every indicator there has to be dark to clear
///   3:1, and amber cannot go dark without turning brown and colliding with `terminated`.
/// * **Both boards** — `awaiting_user` is the largest mark drawn, which is the halo. That is
///   theme-independent, and it is what carries the summons where the ink index cannot.
///
/// The `idle` end is TC-52a, which holds on both themes. The middle of the ladder is
/// deliberately not an urgency ranking
/// ([ADR-0014](../../../docs/adr/0014-the-ladder-pins-the-ends-not-the-middle.md)), so
/// nothing asserts one.
#[test]
fn tc_52c_the_loud_end_holds() {
    let loudest = Status::ALL
        .into_iter()
        .max_by(|a, b| ink_index(*a, Theme::Dark).total_cmp(&ink_index(*b, Theme::Dark)))
        .expect("there is at least one status");
    assert_eq!(
        loudest,
        Status::AwaitingUser,
        "the loudest thing on the dark board is {}, not the summons",
        loudest.name(),
    );

    let summons = indicator_area(Status::AwaitingUser);
    for status in Status::ALL {
        if status == Status::AwaitingUser {
            continue;
        }
        let other = indicator_area(status);
        assert!(
            summons > other,
            "the summons is drawn at {summons:.0} px² and {} at {other:.0}; the halo has to \
             make `awaiting_user` the largest mark on either theme",
            status.name(),
        );
    }
}

// ---------------------------------------------------------------- TC-53 / TC-54, FR-17

/// TC-53 — the shape says parent or child, never which status.
///
/// **This case used to assert the opposite**, and the reversal is FR-17's, amended
/// 2026-08-27: six silhouettes were six things to learn on a board that is glanced at, and
/// the palette was derived to carry the status by itself
/// ([ADR-0024](../../../docs/adr/0024-colour-carries-the-status-shape-carries-the-hierarchy.md)).
/// So the assertion now runs the other way — a build that gave `terminated` a shape of its
/// own would fail here, because that shape would start meaning something the user has to
/// learn.
///
/// The load that moved off this case landed on TC-49 … TC-51, which measure the palette's
/// separations under greyscale and three colour-vision simulations. Those are no longer belt
/// *and* braces; they are the belt.
#[test]
fn tc_53_the_shape_says_parent_or_child_never_which_status() {
    for status in Status::ALL {
        let drawn = rendering(status).silhouette;
        let expected = if status == Status::Unknown {
            // The one exception, and it is about ink rather than identification: `unknown`
            // must carry the least of the six (§3.2), and a filled disc at 5.05:1 would make
            // the observer's admission that it cannot read a session the third loudest thing
            // on the board.
            Silhouette::DottedRing
        } else {
            Silhouette::FilledDisc
        };
        assert_eq!(
            drawn,
            expected,
            "{} is drawn as {drawn:?}; the shape must not encode which status this is",
            status.name(),
        );
    }
}

/// The hollow ring belongs to the hierarchy, and to nothing else.
///
/// It is what an *open* workspace's mark is drawn as, so that a parent and the sessions
/// underneath it can be told apart (§3.1). If a status ever took it, the two meanings would
/// collide on one shape and the board would be back to teaching silhouettes.
#[test]
fn tc_53b_the_hollow_ring_is_never_a_status() {
    for status in Status::ALL {
        assert_ne!(
            rendering(status).silhouette,
            Silhouette::HollowRing,
            "{} took the shape reserved for an open workspace's own mark",
            status.name(),
        );
        assert_ne!(
            reduced_motion_rendering(status).silhouette,
            Silhouette::HollowRing,
            "{} takes it under reduced motion",
            status.name(),
        );
    }
}

/// TC-54 — every animated status has a static substitute.
///
/// "Static" is read as *not animated*: `limited` keeps a level that steps every ten
/// minutes, which is a substitute rather than an animation.
///
/// What it can no longer assert is that the six stay distinguishable with the motion turned
/// off, because that was the silhouettes' job and they no longer have it. With reduced motion
/// on, the separation is the palette's alone — TC-49, TC-50 and TC-51 — and the case says so
/// rather than quietly checking something weaker.
#[test]
fn tc_54_every_animated_status_has_a_static_substitute() {
    let mut animated = 0;
    for status in Status::ALL {
        if !rendering(status).motion.is_animated() {
            continue;
        }
        animated += 1;
        let reduced = reduced_motion_rendering(status);
        assert!(
            !reduced.motion.is_animated(),
            "{} still animates under reduced motion ({:?})",
            status.name(),
            reduced.motion,
        );
        assert_eq!(
            reduced.silhouette,
            rendering(status).silhouette,
            "{} changes shape under reduced motion, which would make the shape mean \
             something again",
            status.name(),
        );
    }
    assert!(animated > 0, "no status has motion to substitute for");

    // And the summons keeps something to be substituted *with*. Turning the motion off must
    // not turn `awaiting_user` into an ordinary disc: the wave is what makes it the largest
    // mark on the board, which is one of the two ends ADR-0014 pins, and TC-52c reads that
    // area from `indicator_area`. A reduced-motion user must still get the loudest thing.
    let summons = reduced_motion_rendering(Status::AwaitingUser);
    assert_ne!(
        summons.motion,
        Motion::None,
        "with the motion gone, the summons is a filled disc like every other status and          nothing but colour is left to say the developer has to move",
    );
    assert!(
        !summons.motion.is_animated(),
        "...and it must not still be animating: {:?}",
        summons.motion,
    );
}
