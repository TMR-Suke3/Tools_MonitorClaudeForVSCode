//! TC-29 … TC-36 — fitting a title to its row (FR-19 … FR-22).
//!
//! The recording is `titles.json`: widths measured from **98 distinct real titles**, with the
//! text synthesised to match each one's character count and cell width exactly. Rows marked
//! `derived` are not measured — they exist to carry the grapheme-cluster cases that do not
//! occur in the corpus.
//!
//! ## One thing about that fixture's `cells`
//!
//! Its `cells` values were computed per code point: 2 for East Asian Wide or Fullwidth, 1 for
//! everything else, with no zero-width handling. For the seven **measured** rows — Latin and
//! CJK only — that is exactly the rendered width, and this suite checks its own width
//! function against them.
//!
//! For three of the derived rows it is not. A ZWJ emoji sequence renders as one glyph of two
//! cells, not as three emoji and two joiners; a combining acute and a variation selector
//! render at zero width. The fixture says those rows are not measured, and it is right to:
//!
//! | row | fixture `cells` | rendered |
//! |---|---|---|
//! | `family 👩‍👩‍👧 photo` | 21 | 15 |
//! | `café latte order` | 17 | 16 |
//! | `🖥️ monitor setup` | 16 | 15 |
//!
//! So `cells` is asserted **only** for the measured rows, and the derived rows are used for
//! what they were cut for: proving a cut never lands inside a cluster. The fixture is not
//! edited — it is a recording, and rule 1 of `docs/test-plan.md` §1 applies.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::record::{Opening, human_prompt_line};
use mcv_core::title::{ELLIPSIS, Fitted, Sources, fit, fit_best, normalise, width};

struct Row {
    title: String,
    chars: usize,
    cells: usize,
    derived: bool,
}

fn rows() -> Vec<Row> {
    let doc: serde_json::Value =
        serde_json::from_str(&fixtures::read("titles.json")).expect("the fixture is JSON");
    doc["titles"]
        .as_array()
        .expect("the fixture holds titles")
        .iter()
        .map(|row| Row {
            title: row["title"].as_str().expect("a title").to_owned(),
            chars: row["chars"].as_u64().expect("chars") as usize,
            cells: row["cells"].as_u64().expect("cells") as usize,
            derived: row["derived"].as_bool().unwrap_or(false),
        })
        .collect()
}

fn measured() -> Vec<Row> {
    rows().into_iter().filter(|r| !r.derived).collect()
}

// ------------------------------------------------ FR-19

/// TC-29 — titles pass through unmodified whenever they fit.
///
/// No summarising, no translating, no word-stripping. Claude Code already did the
/// summarising, and doing it again without a model is how a board starts lying.
#[test]
fn tc_29_titles_pass_through_unmodified() {
    for row in rows() {
        let fitted = fit(&row.title, width(&row.title));
        assert_eq!(
            fitted.fitted, row.title,
            "a title that fits must come back exactly as it went in",
        );
        assert!(!fitted.truncated);
        assert!(
            !fitted.fitted.contains(ELLIPSIS) || row.title.contains(ELLIPSIS),
            "nothing may be marked as cut when nothing was cut",
        );
    }
}

/// TC-30 — the fallback chain always yields something.
///
/// A row nobody can identify is the one thing this board exists to prevent, so the chain
/// ends at the workspace folder, which always exists.
#[test]
fn tc_30_the_fallback_chain_always_yields_something() {
    let empty = rows()
        .into_iter()
        .find(|r| r.title.is_empty())
        .expect("the recording carries an empty title on purpose");

    let full_chain = fit_best(
        Sources {
            ai_title: Some(&empty.title),
            registry_name: Some("sample-repo-02"),
            prompt_line: Some("a prompt"),
            derived_name: None,
            workspace_label: "sample-repo",
        },
        40,
    );
    assert_eq!(
        full_chain.fitted, "sample-repo-02",
        "an empty ai-title falls through to the registry's name",
    );

    let nothing_but_the_folder = fit_best(
        Sources {
            ai_title: None,
            registry_name: Some("   "),
            prompt_line: None,
            derived_name: None,
            workspace_label: "sample-repo",
        },
        40,
    );
    assert_eq!(
        nothing_but_the_folder.fitted, "sample-repo",
        "whitespace is not a name, and the folder is the last resort",
    );
    assert!(
        !fit_best(
            Sources {
                ai_title: None,
                registry_name: None,
                prompt_line: None,
                derived_name: None,
                workspace_label: "sample-repo",
            },
            40,
        )
        .fitted
        .is_empty(),
        "the chain must never produce an unnamed row",
    );
}

// ------------------------------------------------ FR-20

/// TC-31 — truncation is by rendered width, not by character count.
///
/// The recording's own numbers make the point: a 13-character title 18 cells wide and a
/// 14-character title 28 cells wide must cut at different character counts for one budget.
#[test]
fn tc_31_truncation_is_by_rendered_width() {
    let rows = measured();
    let narrow = rows
        .iter()
        .find(|r| r.chars == 13 && r.cells == 18)
        .expect("the recording holds a 13-character, 18-cell title");
    let wide = rows
        .iter()
        .find(|r| r.chars == 14 && r.cells == 28)
        .expect("the recording holds a 14-character, 28-cell title");

    const BUDGET: usize = 12;
    let a = fit(&narrow.title, BUDGET);
    let b = fit(&wide.title, BUDGET);

    let kept = |f: &Fitted| f.fitted.chars().count() - ELLIPSIS.chars().count();
    assert_ne!(
        kept(&a),
        kept(&b),
        "two titles of nearly equal length but very different width cut at the same character \
         count, which means the fitting is counting characters",
    );
    assert!(width(&a.fitted) <= BUDGET);
    assert!(width(&b.fitted) <= BUDGET);
}

/// TC-32 — full-width text is never clipped mid-glyph.
///
/// Every measured row, at every budget from 1 to its full width. A CJK glyph is two cells,
/// so a fitting that counted characters would overflow by one cell somewhere in this sweep.
#[test]
fn tc_32_full_width_text_is_never_clipped() {
    for row in measured() {
        assert_eq!(
            width(&row.title),
            row.cells,
            "the recording measured {:?} at {} cells",
            row.title,
            row.cells,
        );
        for budget in 1..=row.cells {
            let fitted = fit(&row.title, budget);
            assert!(
                width(&fitted.fitted) <= budget,
                "{:?} at budget {budget} rendered {} cells wide: {:?}",
                row.title,
                width(&fitted.fitted),
                fitted.fitted,
            );
        }
    }
}

// ------------------------------------------------ FR-21

/// TC-33 — grapheme clusters survive.
///
/// The derived rows are here for this: a ZWJ emoji family, a combining acute, a variation
/// selector, Hangul and RTL text. At every budget, whatever comes back must still be a
/// sequence of whole clusters of the original — never a cluster split down the middle, and
/// never a lone surrogate.
#[test]
fn tc_33_grapheme_clusters_survive() {
    use unicode_segmentation::UnicodeSegmentation;

    for row in rows() {
        let clusters: Vec<&str> = row.title.graphemes(true).collect();
        for budget in 1..=width(&row.title).max(1) {
            let fitted = fit(&row.title, budget);
            let kept = fitted
                .fitted
                .strip_suffix(ELLIPSIS)
                .unwrap_or(&fitted.fitted);

            for (cursor, grapheme) in kept.graphemes(true).enumerate() {
                assert_eq!(
                    Some(grapheme),
                    clusters.get(cursor).copied(),
                    "{:?} at budget {budget} produced {kept:?}, which is not a prefix of the \
                     original's clusters",
                    row.title,
                );
            }
        }
    }
}

/// TC-34 — a cut is marked, and the mark is inside the budget.
///
/// An ellipsis appended *past* the budget would overflow the row by exactly the width of the
/// thing that was supposed to prevent overflowing.
#[test]
fn tc_34_a_cut_is_marked_within_the_budget() {
    let mut checked = 0;
    for row in rows() {
        let full = width(&row.title);
        if full <= width(ELLIPSIS) {
            continue;
        }
        for budget in width(ELLIPSIS)..full {
            let fitted = fit(&row.title, budget);
            assert!(fitted.truncated);
            assert!(
                fitted.fitted.ends_with(ELLIPSIS),
                "{:?} at budget {budget} was cut without saying so: {:?}",
                row.title,
                fitted.fitted,
            );
            assert!(
                width(&fitted.fitted) <= budget,
                "the mark must fit inside the budget"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the recording contains titles worth cutting");
}

// ------------------------------------------------ FR-22

/// TC-35 — the full title stays reachable.
///
/// Truncation is a display decision, and the board shows the whole title on hover. A fitting
/// that only returned the cut string would make that impossible.
#[test]
fn tc_35_the_full_title_stays_reachable() {
    for row in rows() {
        let fitted = fit(&row.title, 8);
        assert_eq!(
            fitted.full,
            normalise(&row.title),
            "the original must survive alongside the fitted form",
        );
    }
}

// ------------------------------------------------ FR-19, in-session

/// TC-36 — a retitle is not a new session.
///
/// `title-change.jsonl` holds two different `aiTitle` values in one session. The identity is
/// the session id, so the entry updates in place: nothing about a new title makes a new row.
#[test]
fn tc_36_a_retitle_is_not_a_new_session() {
    use mcv_core::record::Record;

    let mut titles = Vec::new();
    let mut ids = Vec::new();
    for (offset, line) in fixtures::lines("title-change.jsonl") {
        let entry = mcv_core::record::parse_line(&line, offset).expect("the recording parses");
        if let Record::AiTitle(title) = &entry.record {
            titles.push(title.clone());
            if let Some(id) = &entry.meta.session_id {
                ids.push(id.clone());
            }
        }
    }

    assert!(titles.len() >= 2, "the recording retitles the session");
    assert_ne!(titles[0], titles[1], "the two titles differ");
    ids.dedup();
    assert_eq!(
        ids.len(),
        1,
        "both titles belong to one session; a retitle changes the label, not the identity",
    );
}

// ------------------------------------------------ §4, step 1

/// Normalising, which the specification lists first and which the fixture cannot show:
/// titles arrive as JSON strings and can carry newlines, tabs and runs of spaces, and a
/// board row is one line.
#[test]
fn whitespace_is_normalised_and_control_characters_are_dropped() {
    assert_eq!(normalise("  fix   the\tparser  "), "fix the parser");
    assert_eq!(normalise("two\nlines"), "two lines");
    assert_eq!(normalise("bell\u{7}inside"), "bellinside");
    assert_eq!(normalise("   "), "");
    // Normalising must not disturb what it is not about.
    assert_eq!(normalise("あいうえお 設定"), "あいうえお 設定");
}

/// A budget with no room even for the mark of truncation yields nothing, rather than an
/// ellipsis drawn past the edge of the row.
#[test]
fn a_budget_too_small_for_the_mark_yields_nothing() {
    let fitted = fit("あいうえお", 0);
    assert_eq!(fitted.fitted, "");
    assert!(fitted.truncated);
    assert_eq!(fitted.full, "あいうえお", "the original survives even so");
}

// ------------------------------------------------- what the editor's tab is named

/// A `user` record as the transcript writes one, cut to the fields this reads.
/// A prompt whose blocks are given in order, so a case can put the editor's own note first.
fn human_prompt_blocks(blocks: &[(&str, &str)]) -> String {
    let inner: Vec<String> = blocks
        .iter()
        .map(|(kind, text)| {
            format!(
                r#"{{"type":{},"text":{}}}"#,
                serde_json::to_string(kind).expect("a JSON string"),
                serde_json::to_string(text).expect("a JSON string")
            )
        })
        .collect();
    format!(
        r#"{{"type":"user","origin":{{"kind":"human"}},"message":{{"role":"user","content":[{}]}}}}"#,
        inner.join(",")
    )
}

fn human_prompt(text: &str) -> String {
    format!(
        r#"{{"type":"user","origin":{{"kind":"human"}},"message":{{"role":"user","content":[{{"type":"text","text":{}}}]}}}}"#,
        serde_json::to_string(text).expect("a JSON string")
    )
}

/// TC-111 (FR-19) — the opening prompt is read, and only from a human `user` record.
///
/// The editor names a session's tab after the generated title once there is one, and after
/// the opening prompt until then (ADR-0030). A board showing a different name from the tab is
/// one the user has to translate.
#[test]
fn tc_111_the_opening_human_prompt_is_read() {
    assert_eq!(
        human_prompt_line(&human_prompt("メモを整理して"), 120),
        Opening::Line("メモを整理して".to_owned())
    );
}

/// TC-111b — **a tool result is a `user` record too**, and its content is neither a prompt
/// nor a name. `origin.kind` is what tells them apart, and anything that is not `human` is
/// not read at all.
#[test]
fn tc_111b_only_a_human_origin_is_read() {
    let tool_result = r#"{"type":"user","message":{"content":[{"type":"text","text":"exit 0"}]}}"#;
    assert_eq!(human_prompt_line(tool_result, 120), Opening::NotAPrompt);

    let machine = r#"{"type":"user","origin":{"kind":"tool"},"message":{"content":[{"type":"text","text":"exit 0"}]}}"#;
    assert_eq!(human_prompt_line(machine, 120), Opening::NotAPrompt);

    let assistant = r#"{"type":"assistant","origin":{"kind":"human"},"message":{"content":[{"type":"text","text":"no"}]}}"#;
    assert_eq!(human_prompt_line(assistant, 120), Opening::NotAPrompt);
    assert_eq!(human_prompt_line("not json", 120), Opening::NotAPrompt);
}

/// TC-111c — the **first line**, capped. A title is a label, and a prompt that runs to a
/// paragraph has said what a label needs before its first newline. The cap is what keeps the
/// product to one short line of what the user wrote (NFR-07).
#[test]
fn tc_111c_only_the_first_line_and_only_so_much_of_it() {
    let line_of = |opening: Opening| match opening {
        Opening::Line(line) => line,
        other => panic!("expected a line, got {other:?}"),
    };

    let many = human_prompt("first line\nsecond line\nthird");
    assert_eq!(line_of(human_prompt_line(&many, 120)), "first line");

    // **A bare carriage return is a line break too.** `str::lines` does not treat one as a
    // boundary, so a prompt written on a machine that uses them came back whole — several
    // lines held where the promise says one.
    let old_mac = human_prompt("first line\rsecond line");
    assert_eq!(line_of(human_prompt_line(&old_mac, 120)), "first line");

    let long = human_prompt(&"あ".repeat(400));
    let got = line_of(human_prompt_line(&long, 120));
    assert_eq!(got.chars().count(), 120, "capped by characters, not bytes");
}

/// TC-111d — **a human prompt that says nothing is still a human prompt.**
///
/// It has been *seen*, and the caller must stop looking. Folding this in with "not a prompt"
/// would let the search run on to the second prompt and the third until one had text, and the
/// stored line would be "the first prompt that parsed" rather than the opening one — the
/// promise of reading one prompt per session would quietly stop being a promise (ADR-0030).
#[test]
fn tc_111d_a_prompt_with_no_line_still_ends_the_looking() {
    assert_eq!(
        human_prompt_line(&human_prompt("   \t "), 120),
        Opening::NoLine
    );
    assert_eq!(human_prompt_line(&human_prompt(""), 120), Opening::NoLine);

    // A human prompt carrying no text block at all — an image, say.
    let image =
        r#"{"type":"user","origin":{"kind":"human"},"message":{"content":[{"type":"image"}]}}"#;
    assert_eq!(human_prompt_line(image, 120), Opening::NoLine);
}

/// TC-112 (FR-19) — the order the board picks a name in is the editor's own.
///
/// A name somebody chose outranks everything. Then the generated title, then the opening
/// prompt — and the registry's *derived* slug below both, because `notes-e8` is not a name
/// anybody wrote. It stays above the workspace label only because it tells two sessions of
/// one workspace apart.
#[test]
fn tc_112_the_opening_prompt_outranks_the_derived_slug() {
    let fitted = fit_best(
        Sources {
            ai_title: None,
            registry_name: None,
            prompt_line: Some("メモを整理して"),
            derived_name: Some("notes-e8"),
            workspace_label: "notes",
        },
        30,
    );
    assert_eq!(fitted.full, "メモを整理して");

    // ...and a generated title outranks the prompt, which is what the tab does too.
    let titled = fit_best(
        Sources {
            ai_title: Some("トレイアイコンと診断パネル"),
            registry_name: None,
            prompt_line: Some("メモを整理して"),
            derived_name: Some("notes-e8"),
            workspace_label: "notes",
        },
        30,
    );
    assert_eq!(titled.full, "トレイアイコンと診断パネル");

    // With neither, the slug still beats the bare folder: it tells two sessions apart.
    let untitled = fit_best(
        Sources {
            ai_title: None,
            registry_name: None,
            prompt_line: None,
            derived_name: Some("notes-e8"),
            workspace_label: "notes",
        },
        30,
    );
    assert_eq!(untitled.full, "notes-e8");
}

/// TC-112b — a name **somebody chose** outranks the opening prompt, and sits directly under
/// the generated title.
///
/// **The order of those two is not measured.** Every `nameSource` seen on a live machine was
/// `derived`; no recording carries a session anybody renamed, so what the editor's tab shows
/// for one is unknown. The generated title is kept first because that *is* measured — it is
/// what the tab showed for the one session that had one — and guessing the other way would
/// put an unmeasured claim above a measured one (ADR-0030).
#[test]
fn tc_112b_a_chosen_name_sits_under_the_generated_title() {
    let both = fit_best(
        Sources {
            ai_title: Some("a generated title"),
            registry_name: Some("the name I gave it"),
            prompt_line: Some("an opening prompt"),
            derived_name: None,
            workspace_label: "sample-repo",
        },
        40,
    );
    assert_eq!(both.full, "a generated title");

    // Without a generated title, the chosen name is what shows — above the opening prompt,
    // which is the part this does rest on: a name somebody typed says more than the first
    // thing they happened to ask.
    let chosen = fit_best(
        Sources {
            ai_title: None,
            registry_name: Some("the name I gave it"),
            prompt_line: Some("an opening prompt"),
            derived_name: None,
            workspace_label: "sample-repo",
        },
        40,
    );
    assert_eq!(chosen.full, "the name I gave it");
}

/// The editor's own note is not what the user typed.
///
/// VS Code puts the file on screen, or the lines selected, in a **text block of its own**
/// before the one the user wrote. Reading the first text block therefore named 41 of the 69
/// recorded sessions after that note: a board showing `<ide opened file>The user ope…` where
/// the tab shows the question. Found by running the board, not by reading the code.
#[test]
fn the_editors_note_is_not_the_prompt() {
    let line = human_prompt_blocks(&[
        (
            "text",
            "<ide_opened_file>The user opened the file Untitled-1 in the IDE. This may or may              not be related to the current task.</ide_opened_file>",
        ),
        ("text", "この設定を変えたときの挙動について教えて"),
    ]);
    assert_eq!(
        human_prompt_line(&line, 120),
        Opening::Line("この設定を変えたときの挙動について教えて".to_owned()),
        "the name has to come from the prompt, not from the editor's note above it",
    );
}

/// The same rule, without the tag names.
///
/// `ide_opened_file` and `ide_selection` are the only two in the corpus, and listing them
/// would break the day a third is added -- these are Claude Code internals with no
/// stability promise. What is recognised is the *shape*: a block that is nothing but
/// complete elements.
#[test]
fn a_note_this_build_has_never_seen_is_skipped_too() {
    let line = human_prompt_blocks(&[
        ("text", "<some_future_tag>anything at all</some_future_tag>"),
        ("text", "the actual question"),
    ]);
    assert_eq!(
        human_prompt_line(&line, 120),
        Opening::Line("the actual question".to_owned()),
    );
}

/// A note and nothing else has still been *seen*: `NoLine`, not `NotAPrompt`.
///
/// The session keeps the name it had. Answering `NotAPrompt` would send the caller on to
/// the second prompt and the third, and the stored line would be "the first prompt that
/// parsed" rather than the opening one (ADR-0030).
#[test]
fn a_prompt_that_is_only_a_note_names_nothing() {
    let line = human_prompt_blocks(&[(
        "text",
        "<ide_opened_file>The user opened the file x in the IDE.</ide_opened_file>",
    )]);
    assert_eq!(human_prompt_line(&line, 120), Opening::NoLine);
}

/// Text that merely *contains* a tag is a prompt, and is read whole.
///
/// The rule is "nothing but complete elements", not "mentions a tag" -- someone asking
/// about `<div>` is asking a question, and it is the one they should see on the board.
#[test]
fn a_prompt_that_mentions_a_tag_is_still_a_prompt() {
    let line = human_prompt_blocks(&[("text", "why does <div> break here?")]);
    assert_eq!(
        human_prompt_line(&line, 120),
        Opening::Line("why does <div> break here?".to_owned()),
    );
    // An unclosed element is not one this can make sense of, so the text is left alone --
    // failing towards "this is a prompt" rather than towards discarding it.
    let ragged = human_prompt_blocks(&[("text", "<not-closed and then some words")]);
    assert_eq!(
        human_prompt_line(&ragged, 120),
        Opening::Line("<not-closed and then some words".to_owned()),
    );
}

/// The case that already worked, kept so it cannot quietly stop working: a block whose
/// `type` is not `text` was always skipped, and an image is the ordinary way that happens.
#[test]
fn an_image_before_the_prompt_still_yields_the_prompt() {
    let line = human_prompt_blocks(&[
        ("image", ""),
        ("text", "現在のデバイス構成について相談する"),
    ]);
    assert_eq!(
        human_prompt_line(&line, 120),
        Opening::Line("現在のデバイス構成について相談する".to_owned()),
    );
}
