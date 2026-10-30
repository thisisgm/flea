use super::*;

fn line(family: &str, style: &str, bytes: u64, glyphs: u64) -> String {
    facts_line(&crate::i18n::test_catalogs::english(), family, style, bytes, glyphs)
}

fn size(bytes: u64) -> String {
    crate::i18n::test_catalogs::english().locale().format_size(bytes)
}

// Ported from tests/js/fontfacts.js.
#[test]
fn font_facts_line_drops_what_the_font_lacks() {
    let dot = " \u{b7} ";
    let s = size(312_000);
    assert_eq!(
        line("Inter", "Bold", 312_000, 2548),
        format!("Inter{dot}Bold{dot}{s}{dot}2,548 glyphs"),
        "a whole font reads family, style, size and the grouped glyph count"
    );
    assert_eq!(
        line("Inter", "Bold", 312_000, 1),
        format!("Inter{dot}Bold{dot}{s}{dot}1 glyph"),
        "one glyph is singular"
    );
    assert_eq!(
        line("", "Bold", 312_000, 2548),
        format!("Bold{dot}{s}{dot}2,548 glyphs"),
        "a font with no family drops it"
    );
    assert_eq!(
        line("Inter", "", 312_000, 2548),
        format!("Inter{dot}{s}{dot}2,548 glyphs"),
        "a font with no style drops it"
    );
    assert_eq!(
        line("Inter", "Bold", 312_000, 0),
        format!("Inter{dot}Bold{dot}{s}"),
        "a font with no glyph count drops it"
    );
    assert_eq!(
        line("", "", 312_000, 0),
        s,
        "a font with no tables states its size alone and never Unknown"
    );
    assert_eq!(
        line("A", "B", 1000, 1204),
        format!("A{dot}B{dot}{}{dot}1,204 glyphs", size(1000)),
        "a four-figure count is grouped"
    );
    assert_eq!(
        line("Adwaita Sans", "Regular", 879_800, 2937),
        "Adwaita Sans \u{b7} Regular \u{b7} 879.8 kB \u{b7} 2,937 glyphs",
        "the ql-font board's facts"
    );
}

#[test]
fn font_sample_is_the_boards_sentence_at_three_sizes() {
    assert_eq!(
        format!("{SAMPLE_TEXT}|{SAMPLE_SIZES:?}"),
        "The quick brown fox jumps over the lazy dog|[22, 15, 12]"
    );
    assert_eq!(
        physical(1., 1.),
        [22, 15, 12],
        "at a ratio of one the sizes are the board's"
    );
    assert_eq!(
        physical(14. / 12., 1.25),
        [33, 23, 18],
        "the text-size ratio rounds first, then the pixel ratio"
    );
}

#[test]
fn font_kind_rule() {
    let fonts = ["a.ttf", "A.OTF", "b.ttc", "x.tar.ttf"];
    assert!(
        fonts.iter().all(|n| is_font(n)),
        "TrueType, OpenType and collections open the font page, whatever the case"
    );
    let not = ["a.woff", "a.woff2", "a.pfb", "ttf", ".ttf", "notes.txt"];
    assert!(
        not.iter().all(|n| !is_font(n)),
        "WOFF, WOFF2, Type 1 and bare extensions stay unsupported"
    );
}

fn page(path: &str, size: u64) -> Page {
    Page {
        active: true,
        path: path.into(),
        size,
        ..Page::default()
    }
}

fn good(id: u64, prefix: &str, family: &str) -> Reply {
    Reply {
        id,
        err: None,
        family: family.into(),
        style: "Regular".into(),
        glyphs: 1500,
        files: (0..3).map(|i| format!("/c/{prefix}-{i}.png")).collect(),
    }
}

// tests/fontpane.qml: nothing draws before the answer, then all three samples and the facts land at once.
#[test]
fn font_page_draws_whole_on_its_own_answer() {
    let mut p = page("/f/Inter-Bold.ttf", 312_000);
    let ask = p
        .ask("#c0caf5", "#1a1b26", [22, 15, 12])
        .expect("the page asks");
    assert_eq!(
        (ask.path.as_str(), ask.text, ask.sizes),
        ("/f/Inter-Bold.ttf", SAMPLE_TEXT, [22, 15, 12]),
        "the ask carries path, colours, sizes and the sentence"
    );
    assert!(
        !p.drawn() && p.facts_line(&crate::i18n::test_catalogs::english()).is_empty(),
        "the page is not partly up before the answer"
    );
    assert_eq!(
        p.ask("#c0caf5", "#1a1b26", [22, 15, 12]),
        None,
        "the same key asks once"
    );
    assert!(p.answered(good(ask.id, "r0", "Inter")));
    assert!(
        p.drawn() && p.look.as_ref().unwrap().reply.files.len() == 3,
        "the answer swaps all three samples at once"
    );
}

// A theme change asks once with both colours new and keeps the old samples up until the new three replace them.
#[test]
fn font_theme_change_keeps_the_old_page_until_the_new_lands() {
    let mut p = page("/f/a.ttf", 1);
    let first = p.ask("#ffffff", "#000000", [22, 15, 12]).unwrap();
    p.answered(good(first.id, "r0", "A"));
    let second = p
        .ask("#000000", "#ffffff", [22, 15, 12])
        .expect("a theme change asks again");
    assert!(
        p.drawn(),
        "the old page stays up while the new colours are drawn"
    );
    assert_eq!(p.asks, 2, "one ask with both colours new");
    p.answered(good(second.id, "r1", "A"));
    assert!(
        p.look.as_ref().unwrap().reply.files[0].contains("r1"),
        "the new three replace the old at once"
    );
}

// A superseded ask's answer and an earlier page's late answer are dropped.
#[test]
fn font_stale_answers_are_dropped() {
    let mut p = page("/f/b.ttf", 2000);
    let slow = p.ask("#fff", "#000", [22, 15, 12]).unwrap();
    p.path = "/f/c.otf".into();
    p.size = 3000;
    let quick = p.ask("#fff", "#000", [22, 15, 12]).unwrap();
    assert!(
        !p.drawn(),
        "a font whose answer is pending does not show the last font's page"
    );
    assert!(p.answered(good(quick.id, "hack", "Hack")));
    assert!(
        !p.answered(good(slow.id, "slow", "Slow")),
        "the slow answer is dropped"
    );
    assert_eq!(
        p.facts_line(&crate::i18n::test_catalogs::english()),
        format!(
            "Hack \u{b7} Regular \u{b7} {} \u{b7} 1,500 glyphs",
            size(3000)
        ),
        "a stale answer never replaces the page on screen"
    );
    let mut next = page("/f/c.otf", 3000);
    let own = next.ask("#fff", "#000", [22, 15, 12]).unwrap();
    assert!(
        own.id > quick.id,
        "ids come from one count shared by every page"
    );
    assert!(
        !next.answered(good(quick.id, "late", "Late")),
        "an earlier page's late answer is never drawn by the next page"
    );
}

// A refused font draws the unsupported sentence and no page, whether or not a page was drawn before.
#[test]
fn font_refusal_leaves_only_the_sentence() {
    let mut p = page("/f/broken.ttf", 100);
    let ask = p.ask("#fff", "#000", [22, 15, 12]).unwrap();
    assert!(p.answered(Reply {
        id: ask.id,
        err: Some("the font tool exited with status 1".into()),
        ..Reply::default()
    }));
    assert!(p.failed() && !p.drawn(), "a refused font shows no page");
    assert_eq!(
        p.refusal(),
        Some(crate::ui_logic_pv2::refusal::Refusal::FontUnsupported),
        "it draws the unsupported state"
    );
    let mut drawn = page("/f/a.ttf", 1);
    let first = drawn.ask("#fff", "#000", [22, 15, 12]).unwrap();
    drawn.answered(good(first.id, "r0", "A"));
    let again = drawn.ask("#000", "#fff", [22, 15, 12]).unwrap();
    drawn.answered(Reply {
        id: again.id,
        err: Some("x".into()),
        ..Reply::default()
    });
    assert!(
        drawn.failed() && drawn.look.is_none(),
        "a refusal after a drawn page leaves only the unsupported state"
    );
    let mut short = page("/f/a.ttf", 1);
    let ask = short.ask("#fff", "#000", [22, 15, 12]).unwrap();
    short.answered(Reply {
        id: ask.id,
        files: vec!["/c/only.png".into()],
        ..Reply::default()
    });
    assert!(
        short.failed(),
        "an answer without three samples is refused, never drawn in part"
    );
}

#[test]
fn font_inactive_or_empty_asks_nothing() {
    let mut off = page("/f/a.ttf", 1);
    off.active = false;
    assert_eq!(
        off.ask("#fff", "#000", [22, 15, 12]),
        None,
        "a closed Quick Look sends no fontlook"
    );
    let mut empty = page("", 1);
    assert_eq!(
        empty.ask("#fff", "#000", [22, 15, 12]),
        None,
        "no path asks nothing"
    );
}

// The recipe: 40 px at the sides, samples 18 px apart, the block centred in the body under the bar, never closer than 24.
#[test]
fn font_page_layout() {
    assert_eq!(
        layout((902., 547.), 160., 1.),
        (40., 822., 194., 18.),
        "the board's block is centred in the body: a 574 px card less its 27 px bar"
    );
    assert_eq!(
        layout((902., 173.), 160., 1.),
        (40., 822., 24., 18.),
        "a short body keeps the least gap"
    );
    assert_eq!(
        layout((902., 547.), 160., 14. / 12.),
        (47., 808., 194., 21.),
        "the ratio scales the sides and the gap"
    );
}

// The card's page sits in the body under its 27 px bar: a 545 px body holds a 130 px block centred at 208, 235 from the card's top, as ql-font.html draws it.
#[test]
fn font_page_centres_in_the_body_without_adding_the_bar_again() {
    let (_, _, top, _) = layout((902., 545.), 130., 1.);
    assert_eq!(27. + top, 235., "the board's first sample sits 235 from the card's top");
}
