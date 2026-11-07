//! The shared field contract on its own: privacy, masked geometry and bidi carets from a real shaper.
use super::{right_to_left, Entry, Kind};
use crate::path::field::{ime_bounds, ime_index, Line};
use gpui::{point, px, size, Bounds, GlyphRun, TestAppContext, TestDispatcher, Window};
use std::rc::Rc;

#[test]
fn vf_more_undo_treats_ime_composition_as_one_edit() {
    for commit in [true, false] {
        let mut entry = Entry::default();
        entry.begin("notes.txt");
        entry.move_to(0);
        entry.select_to(5);
        assert!(entry.ime_mark(None, "n", None));
        assert!(entry.ime_mark(None, "ni", None));
        if commit {
            assert!(entry.ime_replace(None, "\u{4f60}"));
            assert_eq!(entry.text, "\u{4f60}.txt");
        } else {
            assert!(entry.ime_unmark());
            assert_eq!(entry.text, "ni.txt");
        }
        entry.undo();
        assert_eq!(entry.text, "notes.txt");
        assert_eq!(entry.selected, 0..5);
        assert!(entry.marked.is_none());
        assert!(!entry.can_undo());
    }
}

#[test]
fn vf_more_entry_undo_restores_text_and_selection_without_retaining_private_text() {
    let mut entry = Entry::default();
    entry.begin("notes.txt");
    entry.move_to(0);
    entry.select_to(5);
    entry.replace(None, "draft");
    entry.undo();
    assert_eq!(entry.text, "notes.txt");
    assert_eq!(entry.selected, 0..5);
    assert!(!entry.can_undo());
    for _ in 0..80 {
        entry.replace(None, "x");
    }
    assert_eq!(entry.history.len(), 64);
    entry.close();
    assert!(!entry.can_undo());
    entry.kind = Kind::Private;
    entry.begin("private");
    entry.replace(None, "changed");
    assert!(!entry.can_undo());
}

fn shaped(window: &mut Window, text: &str) -> Line {
    let run = window.text_style().to_run(text.len());
    let layout = window
        .text_system()
        .layout_line(text, px(14.), &[run], None);
    let mut line = Line {
        run: Some(Rc::new(GlyphRun::new(layout))),
        ..Line::default()
    };
    line.set_text_direction(text);
    line
}

#[gpui::test]
fn i50_ime_password_surrounding_private(cx: &mut TestAppContext) {
    let visual = cx.add_empty_window();
    let mut entry = Entry {
        kind: Kind::Private,
        ..Entry::default()
    };
    entry.begin("pa\u{1d11e}s");
    let mut adjusted = None;
    assert_eq!(entry.ime_text(&(0..5), &mut adjusted), None);
    assert_eq!(adjusted, None, "a private line reveals no range either");
    assert_eq!(entry.display(), "\u{2022}".repeat(4));
    assert_eq!(entry.copyable(), None, "no secret reaches a clipboard");
    entry.move_to(entry.text.len());
    assert!(
        !entry.composes() && !entry.ime_mark(None, "\u{3071}", None),
        "a password takes no composition"
    );
    assert!(entry.ime_replace(None, "\u{3071}"));
    assert_eq!(entry.ime_marked(), None);
    let line = visual.update(|window, _| shaped(window, &entry.display()));
    let bullet = line.caret_x("\u{2022}".len());
    let field = Bounds::new(point(px(10.), px(4.)), size(px(300.), px(20.)));
    let mark = ime_bounds(&line, &entry, &(5..6), field);
    let near = |a: gpui::Pixels, b: gpui::Pixels| (a - b).abs() < px(0.01);
    assert!(
        bullet > px(0.) && near(mark.size.width, bullet),
        "masked, never the whole field"
    );
    assert!(near(mark.left(), field.left() + bullet * 4.) && mark.top() == field.top());
    assert_eq!(ime_index(&line, &entry, bullet * 4. - px(0.5)), 5);
    entry.close();
    assert!(entry.text.is_empty(), "a closed private line keeps nothing");
}

#[test]
fn ime_numbers_refuse_composition_and_consumers_read_committed_text() {
    let mut number = Entry {
        kind: Kind::Digits,
        ..Entry::default()
    };
    number.begin("445");
    assert!(!number.composes());
    assert!(
        !number.ime_mark(None, "\u{65e5}", None),
        "a number takes no composition"
    );
    assert!(
        !number.ime_replace(None, "\u{65e5}\u{672c}"),
        "a number takes no CJK text"
    );
    assert!(!number.ime_replace(None, "8a"));
    assert_eq!(
        number.text, "445",
        "a refused input leaves the number as it was"
    );
    assert!(number.ime_replace(None, "8080"));
    assert_eq!(number.text, "8080");
    number.begin("");
    assert_eq!(
        number.kind,
        Kind::Digits,
        "a new session keeps the field's kind"
    );

    let mut host = Entry::default();
    host.begin("nas");
    host.move_to(3);
    assert!(host.composes());
    assert!(host.ime_mark(None, "\u{306b}\u{307b}", None));
    assert_eq!(
        host.text, "nas\u{306b}\u{307b}",
        "the field still paints its preedit"
    );
    assert_eq!(
        host.committed(),
        "nas",
        "a consumer never reads the preedit"
    );
    assert!(host.ime_replace(None, "\u{65e5}"));
    assert_eq!(host.committed(), "nas\u{65e5}");
    host.begin("\u{65e5}\u{672c}");
    host.move_to("\u{65e5}".len());
    assert!(host.ime_mark(None, "\u{306b}", None));
    assert_eq!(host.text, "\u{65e5}\u{306b}\u{672c}");
    assert_eq!(
        host.committed(),
        "\u{65e5}\u{672c}",
        "a preedit inside the line keeps both sides"
    );
}

#[test]
fn i50_ime_bidi_caret_bounds() {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let visual = cx.add_empty_window();
    let text = "ab\u{5e9}\u{5dc}\u{5d5}\u{5dd}cd";
    let (line, rtl) =
        visual.update(|window, _| (shaped(window, text), shaped(window, "\u{5e9}\u{5dc}")));
    let layout = line.run.as_ref().unwrap().line().clone();
    let glyphs = || layout.runs.iter().flat_map(|run| &run.glyphs);
    let x = |at: usize| glyphs().find(|g| g.index == at).unwrap().position.x;
    assert!(x(8) < x(6) && x(6) < x(4) && x(4) < x(2), "a reordered run");
    // A right-to-left character's caret sits on its right edge, the next glyph to its right.
    assert_eq!(
        [0, 1, 2, 4, 8, 10].map(|at| line.caret_x(at)),
        [x(0), x(1), x(10), x(2), x(6), x(10)]
    );
    assert_eq!(line.caret_x(text.len()), layout.width);
    assert_eq!(line.span(&(2..10)), (x(8), x(10)));
    assert_eq!(line.span(&(4..8)), (x(6), x(2)));
    let whole = rtl.run.as_ref().unwrap().line().width;
    assert_eq!((rtl.caret_x(0), rtl.caret_x(4)), (whole, px(0.)));
    let lone = visual.update(|window, _| shaped(window, "\u{5e9}"));
    let one = lone.run.as_ref().unwrap().line().width;
    assert_eq!(
        (lone.caret_x(0), lone.caret_x(2)),
        (one, px(0.)),
        "a lone glyph takes the paragraph's direction"
    );
    let mut entry = Entry::default();
    entry.begin(text);
    let field = Bounds::new(point(px(20.), px(0.)), size(px(400.), px(24.)));
    let mark = ime_bounds(&line, &entry, &(3..5), field);
    assert_eq!(
        (mark.left(), mark.right()),
        (px(20.) + x(6), px(20.) + x(2))
    );
}

#[test]
fn i50_ime_brahmic_clusters_and_paragraph_direction() {
    let mut entry = Entry::default();
    // Malayalam TTTA is a letter, a Devanagari virama links its conjunct, a lone Sinhala al-lakuna does not.
    entry.begin("\u{d15}\u{d3a}\u{915}\u{94d}\u{937}\u{d9a}\u{dca}\u{dc2}");
    assert_eq!([0, 3, 6, 15].map(|at| entry.next(at)), [3, 6, 15, 21]);
    assert_eq!(entry.previous(entry.text.len()), 21);
    // A word-final halant leaves the dot its own stop, a joiner before ASCII binds nothing, Khmer signs ride along.
    entry.begin("\u{924}\u{94d}.a\u{200d}b\u{1780}\u{17cf}");
    assert_eq!([0, 6, 7].map(|at| entry.next(at)), [6, 7, 11]);
    assert_eq!(
        (entry.previous(7), entry.next(12), entry.next(11)),
        (6, 18, 12)
    );
    entry.begin("\u{915}\u{94d}\u{905}\u{915}\u{94d}\u{b95}");
    let stops = [0, 9].map(|at| entry.next(at));
    assert_eq!(stops, [6, 15], "only a consonant of the same script joins");
    entry.begin("\u{9aa}\u{9cd}\u{9f0}\u{b2a}\u{b4d}\u{b71}");
    assert_eq!(
        [0, 9].map(|at| entry.next(at)),
        [9, 18],
        "Assamese RA and Oriya WA are consonants"
    );
    assert!(right_to_left("12 \u{5e9}\u{5dc}") && !right_to_left("ab \u{5e9}"));
    assert!(!right_to_left("\u{2022}\u{2022}") && !right_to_left(""));
    assert!(
        right_to_left("\u{5be}") && right_to_left("\u{661}\u{627}") && !right_to_left("\u{661}a")
    );
    assert!(right_to_left("\u{608}x") && right_to_left("\u{10e60}\u{10900}"));
    let weak_then_latin = [
        "\u{feff}readme",
        "\u{fd3e}note",
        "\u{66a}50 off",
        "\u{600}a",
        "\u{606}x",
        "\u{fd40}n",
        "\u{10e60}a",
        "\u{10a01}a",
        "\u{1e944}a",
        "\u{1eef0}a",
    ];
    assert!(
        weak_then_latin.iter().all(|text| !right_to_left(text)),
        "weak and neutral never decide"
    );
}

fn single_bidi_run(text: &str, positions: &[(usize, f32)], carets: &[(usize, f32)]) {
    use crate::path::{
        entry::Entry,
        field::{ime_bounds, Line},
    };
    use gpui::{
        point, px, size, Bounds, FontId, GlyphId, GlyphRun, LineLayout, ShapedGlyph, ShapedRun,
    };
    use std::{rc::Rc, sync::Arc};

    let mut line = Line {
        run: Some(Rc::new(GlyphRun::new(Arc::new(LineLayout {
            width: px(30.),
            len: text.len(),
            runs: vec![ShapedRun {
                font_id: FontId(0),
                glyphs: positions
                    .iter()
                    .map(|&(index, x)| ShapedGlyph {
                        id: GlyphId(0),
                        position: point(px(x), px(0.)),
                        index,
                        is_emoji: false,
                    })
                    .collect(),
            }],
            ..LineLayout::default()
        })))),
        scroll: px(3.),
        ..Line::default()
    };
    line.set_text_direction(text);
    let mut entry = Entry::default();
    entry.begin(text);
    let bounds = Bounds::new(point(px(50.), px(4.)), size(px(100.), px(20.)));
    for &(index, x) in carets {
        assert_eq!(
            line.caret_x(index),
            px(x),
            "caret at byte {index} in {text:?}"
        );
        assert_eq!(line.span(&(index..index)), (px(x), px(x)));
        let offset = entry.offset_to_utf16(index);
        let ime = ime_bounds(&line, &entry, &(offset..offset), bounds);
        assert_eq!(ime.left(), bounds.left() - line.scroll + px(x));
        assert_eq!(ime.size.width, px(0.));
    }
}

#[test]
fn i50_ime_bidi_single_rtl_start() {
    single_bidi_run(
        "\u{5e9}ab",
        &[(2, 0.), (3, 10.), (0, 20.)],
        &[(0, 30.), (2, 0.), (3, 10.), (4, 20.)],
    );
}

#[test]
fn i50_ime_bidi_single_rtl_middle() {
    single_bidi_run(
        "a\u{5e9}b",
        &[(0, 0.), (1, 10.), (3, 20.)],
        &[(0, 0.), (1, 20.), (3, 20.), (4, 30.)],
    );
}

#[test]
fn i50_ime_bidi_single_rtl_end() {
    single_bidi_run(
        "ab\u{5e9}",
        &[(0, 0.), (1, 10.), (2, 20.)],
        &[(0, 0.), (1, 10.), (2, 30.), (4, 20.)],
    );
}

#[test]
fn i50_ime_bidi_single_ltr_inside_rtl() {
    single_bidi_run(
        "\u{5e9}a\u{5dc}",
        &[(3, 0.), (2, 10.), (0, 20.)],
        &[(0, 30.), (2, 10.), (3, 10.), (5, 0.)],
    );
}

#[test]
fn i50_ime_bidi_levels_follow_overrides_and_clear_on_reshape() {
    let mut line = Line::default();
    line.set_text_direction("a\u{202e}b\u{202c}");
    assert!(!line.right_to_left);
    assert!(line.bidi_levels[4].is_rtl(), "RLO reverses the Latin run");
    line.set_text_direction("\u{5e9}a ");
    assert!(line.right_to_left);
    assert!(line.bidi_levels[2].is_ltr(), "embedded Latin keeps its run");
    assert!(
        line.bidi_levels[3].is_rtl(),
        "trailing space takes the paragraph level"
    );
    line.set_text_direction("a");
    assert_eq!(line.bidi_levels.len(), 1);
    assert!(line.bidi_levels[0].is_ltr());
    line.set_text_direction("");
    assert!(line.bidi_levels.is_empty() && !line.right_to_left);
}

// A press on the last glyph's left half lands before it, as on every other glyph; past its middle it lands at the end.
#[test]
fn r396_index_for_takes_the_nearer_edge_of_the_last_glyph() {
    use crate::path::field::Line;
    use gpui::{point, px, FontId, GlyphId, GlyphRun, LineLayout, ShapedGlyph, ShapedRun};
    use std::{rc::Rc, sync::Arc};

    // Three 10 px glyphs on a 30 px line, given as (byte index, left x) in drawing order.
    let run = |len, glyphs: [(usize, f32); 3]| {
        let glyphs = glyphs.map(|(index, x)| ShapedGlyph {
            id: GlyphId(0),
            position: point(px(x), px(0.)),
            index,
            is_emoji: false,
        });
        let runs = vec![ShapedRun {
            font_id: FontId(0),
            glyphs: glyphs.to_vec(),
        }];
        let layout = LineLayout {
            width: px(30.),
            len,
            runs,
            ..LineLayout::default()
        };
        Some(Rc::new(GlyphRun::new(Arc::new(layout))))
    };
    let mut line = Line {
        run: run(9, [(0, 0.), (3, 10.), (6, 20.)]),
        ..Line::default()
    };
    line.set_text_direction("文件名");
    for (x, index) in [
        (4., 0),
        (9., 3),
        (20.001, 6),
        (24.9, 6),
        (25.1, 9),
        (40., 9),
    ] {
        assert_eq!(line.index_for(px(x)), index, "x {x}");
    }
    // A right-to-left paragraph draws its first letter rightmost, and keeps closest_index_for_x's answer there.
    line.run = run(6, [(4, 0.), (2, 10.), (0, 20.)]);
    line.set_text_direction("\u{5e9}\u{5dc}\u{5dd}");
    assert_eq!(line.index_for(px(21.)), 6);
}
