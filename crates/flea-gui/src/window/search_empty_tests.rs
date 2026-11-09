//! The no-match headline fits its pane: a long query elides with an ellipsis, and the words around it stay whole.
use super::fitted;

/// Every character 10 px wide, the ellipsis included.
fn measure(text: &str) -> f32 {
    10. * text.chars().count() as f32
}

fn english(query: &str) -> String {
    format!("NOTHING MATCHES {query}")
}

fn query_first(query: &str) -> String {
    format!("{query} に一致なし")
}

#[test]
fn a_headline_that_fits_is_drawn_whole() {
    assert_eq!(
        fitted("zzz", english, 1000., measure),
        "NOTHING MATCHES zzz"
    );
    assert_eq!(fitted("zzz", english, 190., measure), "NOTHING MATCHES zzz");
}

#[test]
fn a_long_query_keeps_the_words_and_elides_at_its_end() {
    let query = "q".repeat(300);
    let drawn = fitted(&query, english, 400., measure);
    assert_eq!(drawn, format!("NOTHING MATCHES {}\u{2026}", "q".repeat(23)));
    assert!(measure(&drawn) <= 400.);
    assert!(
        measure(&format!("NOTHING MATCHES {}\u{2026}", "q".repeat(24))) > 400.,
        "the longest head that fits"
    );
}

#[test]
fn words_after_the_query_stay_whole_too() {
    let query = "日本語のとても長い検索語".repeat(10);
    let drawn = fitted(&query, query_first, 200., measure);
    assert!(drawn.ends_with("\u{2026} に一致なし"), "{drawn}");
    assert!(measure(&drawn) <= 200.);
}

#[test]
fn a_pane_too_narrow_for_the_words_elides_the_whole_line() {
    let drawn = fitted("zzz", english, 100., measure);
    assert_eq!(drawn, "NOTHING M\u{2026}");
    assert_eq!(fitted("zzz", english, 5., measure), "\u{2026}");
}

#[test]
fn a_query_of_many_byte_characters_cuts_on_a_character() {
    let query = "\u{1F600}é".repeat(50);
    let drawn = fitted(&query, english, 300., measure);
    assert!(drawn.starts_with("NOTHING MATCHES \u{1F600}é"));
    assert!(drawn.ends_with('\u{2026}'));
    assert!(measure(&drawn) <= 300.);
}
