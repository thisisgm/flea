//! The shared field contract on its own: privacy, masked geometry and bidi carets from a real shaper.
use super::{right_to_left, Entry};
use crate::path::field::{ime_bounds, ime_index, Line};
use gpui::{point, px, size, Bounds, GlyphRun, TestAppContext, TestDispatcher, Window};
use std::rc::Rc;

fn shaped(window: &mut Window, text: &str) -> Line {
    let run = window.text_style().to_run(text.len());
    let layout = window
        .text_system()
        .layout_line(text, px(14.), &[run], None);
    Line {
        run: Some(Rc::new(GlyphRun::new(layout))),
        right_to_left: super::right_to_left(text),
        ..Line::default()
    }
}

#[gpui::test]
fn i50_ime_password_surrounding_private(cx: &mut TestAppContext) {
    let visual = cx.add_empty_window();
    let mut entry = Entry {
        private: true,
        ..Entry::default()
    };
    entry.begin("pa\u{1d11e}s");
    let mut adjusted = None;
    assert_eq!(entry.ime_text(&(0..5), &mut adjusted), None);
    assert_eq!(adjusted, None, "a private line reveals no range either");
    assert_eq!(entry.display(), "\u{2022}".repeat(4));
    assert_eq!(entry.copyable(), None, "no secret reaches a clipboard");
    entry.move_to(entry.text.len());
    assert!(entry.ime_mark(None, "\u{3071}", None));
    assert_eq!(entry.ime_marked(), Some(5..6));
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
    assert!(entry.ime_replace(None, "\u{3071}"));
    entry.close();
    assert!(entry.text.is_empty(), "a closed private line keeps nothing");
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
