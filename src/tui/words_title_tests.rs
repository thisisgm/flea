use super::*;
use crate::backend::testdir::TestDir;
use crate::i18n::Paths;
use crate::tui::cells::{self, text_width};
use crate::tui::render::frame_tests::plain;

const ENGLISH: &str = r#"{"ey.title.moved":"Moved %1 to %2 (%3)","ey.title.twice":"%1, again %1","ey.title.nested":"[%1]","ey.title.items":{"one":"%1 item of %2","other":"%1 items of %2"},"ey.title.word":"folder","ey.title.punct":"«%1»."}"#;
const HEBREW: &str = r#"{"ey.title.moved":"%3 :%2 אל %1 הועבר","ey.title.items":{"other":"%2 מתוך %1 פריטים"},"ey.title.word":"תיקייה"}"#;
const PERSIAN: &str = r#"{"ey.title.items":{"other":"%1 مورد از %2"},"ey.title.punct":"«%1»؟"}"#;

fn language(root: &TestDir, code: &str, english: &str) -> I18n {
    root.dir("omarchy/default/i18n");
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    for (lang, body) in [("en", english), ("he", HEBREW), ("fa", PERSIAN)] {
        root.dir(&format!("catalog/{lang}"));
        root.file(&format!("catalog/{lang}/flea.json"), body);
    }
    root.file("language", code);
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    })
}
fn prose(text: &str) -> Span {
    (None, text.into())
}
fn name(text: &str) -> Span {
    (Some(Run::Name), text.into())
}
fn number(text: &str) -> Span {
    (Some(Run::Number), text.into())
}
fn joined(spans: &[Span]) -> String {
    spans.iter().map(|(_, text)| text.as_str()).collect()
}

#[test]
fn ey_tui_typedtitle_spans_keep_reordered_repeated_and_nested_args_typed() {
    let root = TestDir::new("ey-typedtitle-spans");
    let moved = Text::Args(
        "ey.title.moved",
        vec![
            Text::Data("a.txt".into()),
            Text::Data("/srv/מסמכים".into()),
            Text::Size(2048),
        ],
    );
    let english = language(&root, "en", ENGLISH);
    let size = english.locale().format_size(2048);
    let spans = moved.spans(&english);
    assert_eq!(
        spans,
        vec![
            prose("Moved "),
            name("a.txt"),
            prose(" to "),
            name("/srv/מסמכים"),
            prose(" ("),
            number(&size),
            prose(")")
        ]
    );
    assert_eq!(joined(&spans), moved.render(&english));
    let hebrew = language(&root, "he", ENGLISH);
    let spans = moved.spans(&hebrew);
    assert_eq!(spans[0], number(&hebrew.locale().format_size(2048)));
    assert_eq!(
        &spans[1..],
        &[
            prose(" :"),
            name("/srv/מסמכים"),
            prose(" אל "),
            name("a.txt"),
            prose(" הועבר")
        ]
    );
    assert_eq!(joined(&spans), moved.render(&hebrew));
    let twice = Text::Args("ey.title.twice", vec![Text::Data("x".into())]);
    assert_eq!(
        twice.spans(&english),
        vec![name("x"), prose(", again "), name("x")]
    );
    let nested = Text::Args(
        "ey.title.nested",
        vec![Text::Joined(vec![
            Text::Key("ey.title.word"),
            Text::Data(" b".into()),
            Text::Count(7),
        ])],
    );
    assert_eq!(
        nested.spans(&english),
        vec![prose("[folder"), name(" b"), number("7"), prose("]")]
    );
    assert_eq!(
        nested.spans(&hebrew),
        vec![prose("[תיקייה"), name(" b"), number("7"), prose("]")]
    );
}

#[test]
fn ey_tui_typedtitle_plural_count_and_size_are_number_runs_in_the_locales_digits() {
    let root = TestDir::new("ey-typedtitle-plural");
    for code in ["en", "he", "fa"] {
        let i18n = language(&root, code, ENGLISH);
        for count in [1, 2, 1234] {
            let text = Text::Plural("ey.title.items", count, vec![Text::Size(4096)]);
            let spans = text.spans(&i18n);
            assert_eq!(joined(&spans), text.render(&i18n), "{code} {count}");
            let digits = i18n.locale().format_count(count as i64);
            let size = i18n.locale().format_size(4096);
            let numbers: Vec<&str> = spans
                .iter()
                .filter(|(run, _)| *run == Some(Run::Number))
                .map(|(_, text)| text.as_str())
                .collect();
            assert_eq!(numbers.len(), 2, "{code} {count}: {spans:?}");
            assert!(
                numbers.contains(&digits.as_str()) && numbers.contains(&size.as_str()),
                "{code} {count}: {spans:?}"
            );
            assert!(
                spans.iter().all(|(run, _)| *run != Some(Run::Name)),
                "{code}: {spans:?}"
            );
        }
    }
    assert_eq!(language(&root, "fa", ENGLISH).locale().format_count(3), "۳");
}

#[test]
fn ey_tui_typedtitle_catalog_noncharacters_never_become_holes() {
    let root = TestDir::new("ey-typedtitle-nonchar");
    let english = r#"{"ey.title.moved":"Moved ﷑ %1"}"#;
    let i18n = language(&root, "en", english);
    let text = Text::Args(
        "ey.title.moved",
        vec![Text::Data("a".into()), Text::Data("b".into())],
    );
    assert_eq!(text.spans(&i18n), vec![name(&text.render(&i18n))]);
}

#[test]
fn ey_tui_typedtitle_fit_measures_cells_before_isolating_at_every_width() {
    let spans = vec![
        prose("«"),
        name("界a\u{301}b\u{200d}c"),
        prose("» "),
        number("12"),
    ];
    let total = 1 + 5 + 2 + 2;
    for isolating in [false, true] {
        for bold in [false, true] {
            for mark in ["", "> "] {
                for columns in 0..=14 {
                    let title = typed(mark, &spans, columns, isolating, bold);
                    let at = format!("isolating={isolating} bold={bold} mark={mark:?} columns={columns}: {title:?}");
                    assert_eq!(text_width(&plain(&title)), columns, "{at}");
                    let opens = title.matches(['\u{2066}', '\u{2068}']).count();
                    assert_eq!(opens, title.matches('\u{2069}').count(), "{at}");
                    if !isolating {
                        assert_eq!(opens, 0, "{at}");
                    }
                    assert_eq!(
                        title.matches("\x1b[1m").count(),
                        title.matches("\x1b[22m").count(),
                        "{at}"
                    );
                    if !bold {
                        assert!(!title.contains('\x1b'), "{at}");
                    }
                    let cut = columns > 0 && text_width(mark) + total > columns;
                    assert_eq!(plain(&title).contains('\u{2026}'), cut, "{at}");
                    assert!(
                        !plain(&title).contains('\u{301}') || plain(&title).contains("a\u{301}"),
                        "{at}"
                    );
                }
            }
        }
    }
    assert_eq!(
        typed("", &spans, 14, true, true),
        "\x1b[1m«\u{2068}界a\u{301}b\u{200d}c\u{2069}» \u{2066}12\u{2069}\x1b[22m    "
    );
    // A wide glyph that does not fit leaves a plain blank: unbolded and outside every isolate.
    assert_eq!(
        typed("", &spans, 3, true, true),
        "\x1b[1m«\u{2026}\x1b[22m "
    );
    assert_eq!(
        typed("", &spans, 3, false, true),
        "\x1b[1m«\x1b[22m \x1b[1m\u{2026}\x1b[22m"
    );
    let cut_name = [(Some(Run::Name), "a界b".to_string())];
    assert_eq!(
        typed("", &cut_name, 3, true, true),
        "\x1b[1m\u{2068}a\u{2026}\u{2069}\x1b[22m "
    );
    assert_eq!(
        typed("", &spans, 6, true, true),
        "\x1b[1m«\u{2068}界a\u{301}b\u{200d}\u{2026}\u{2069}\x1b[22m"
    );
}

// The bytes the replaced name_title produced, pinned literally so the shared fitter cannot drift from them.
#[test]
fn ey_tui_typedtitle_name_title_keeps_the_replaced_bytes() {
    for (name, columns, expected) in [
        ("main.rs", 9, "› \x1b[1mmain.rs\x1b[22m"),
        ("main.rs", 12, "› \x1b[1mmain.rs\x1b[22m   "),
        ("main.rs", 6, "› \x1b[1mmai\u{2026}\x1b[22m"),
        ("a界b", 3, "› \u{2026}"),
        ("a界b", 5, "› \x1b[1ma\x1b[22m \x1b[1m\u{2026}\x1b[22m"),
        ("界界.rs", 5, "› \x1b[1m界\u{2026}\x1b[22m"),
        ("", 4, "›   "),
        ("x", 0, ""),
    ] {
        assert_eq!(
            cells::name_title("› ", name, columns),
            expected,
            "{name:?} {columns}"
        );
    }
}

// A cut just after a whole run leaves that run's isolate closed; the ellipsis marks the run cut after it.
#[test]
fn ey_tui_typedtitle_cut_after_a_whole_run_keeps_the_ellipsis_outside_its_isolate() {
    let header = vec![prose("اختير "), number("12"), prose(" عناصر.")];
    assert_eq!(
        typed("", &header, 9, true, true),
        "\x1b[1mاختير \u{2066}12\u{2069}\u{2026}\x1b[22m"
    );
    let named = vec![name("ab"), prose(" cd")];
    assert_eq!(
        typed("", &named, 3, true, true),
        "\x1b[1m\u{2068}ab\u{2069}\u{2026}\x1b[22m"
    );
    let cut_inside = vec![name("abc"), prose(" d")];
    assert_eq!(
        typed("", &cut_inside, 3, true, true),
        "\x1b[1m\u{2068}ab\u{2026}\u{2069}\x1b[22m"
    );
}
