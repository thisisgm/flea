// Settings search, from the settings-search board: no oracle test exists, so these pin the board's rules.
use super::super::rows::{Kind, Row};
use super::super::test_support::english;
use super::super::Text;
use super::*;

fn row(kind: Kind, label: &str) -> Row {
    Row { id: label.to_owned(), ..Row::new(kind, Text::data(label)) }
}

fn sections() -> Vec<(usize, Vec<Row>)> {
    vec![
        (0, vec![row(Kind::Group, "Listing"), row(Kind::Check, "Show Thumbnails"), row(Kind::Check, "Show hidden files"),
            row(Kind::Hint, "Thumbnails load as rows scroll in.")]),
        (3, vec![row(Kind::Group, "Text Size"), row(Kind::Choice, "Text size"), row(Kind::Ruler, ""),
            row(Kind::Fact, "Effective"), row(Kind::Group, "Appearance"), row(Kind::Check, "Hyprland icons")]),
        (5, vec![row(Kind::Hint, "Loose sentence before a heading"), row(Kind::Group, "Preset"), row(Kind::Choice, "Preset")]),
    ]
}

#[test]
fn run_is_case_insensitive_and_counted_in_characters() {
    assert_eq!(run_of("Show Thumbnails", "thumb"), Some(Run { start: 5, length: 5 }));
    assert_eq!(run_of("Größe", "ÖSS"), None);
    assert_eq!(run_of("Größe", "öß"), Some(Run { start: 2, length: 2 }));
    assert_eq!(run_of("Show", "  sh "), Some(Run { start: 0, length: 2 }));
    assert_eq!(run_of("Show", "x"), None);
    assert_eq!(run_of("Show", "   "), None);
}

#[test]
fn blank_query_has_no_results() {
    assert!(search(&sections(), "", &english()).is_empty());
    assert!(search(&sections(), "  ", &english()).is_empty());
}

#[test]
fn rows_and_hints_match_and_name_their_section() {
    let found = search(&sections(), "thumb", &english());
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].section, found[0].heading, found[0].heading_run), (0, Some(0), None));
    let rows: Vec<usize> = found[0].hits.iter().map(|hit| hit.row).collect();
    assert_eq!(rows, [1, 3]);
    assert_eq!(found[0].hits[1].run, Some(Run { start: 0, length: 5 }));
}

#[test]
fn heading_match_brings_its_whole_group_without_washing_rows() {
    let found = search(&sections(), "appear", &english());
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].section, found[0].heading), (3, Some(4)));
    assert_eq!(found[0].heading_run, Some(Run { start: 0, length: 6 }));
    assert_eq!(found[0].hits, [Hit { row: 5, run: None }]);
}

#[test]
fn groups_keep_section_order_and_a_ruler_is_never_washed() {
    let found = search(&sections(), "s", &english());
    let order: Vec<(usize, Option<usize>)> = found.iter().map(|g| (g.section, g.heading)).collect();
    assert_eq!(order, [(0, Some(0)), (3, Some(0)), (3, Some(4)), (5, None), (5, Some(1))]);
    let ruler = found[1].hits.iter().find(|hit| hit.row == 2).expect("the heading brings the ruler");
    assert_eq!(ruler.run, None);
}

#[test]
fn rows_before_any_heading_form_their_own_group() {
    let found = search(&sections(), "loose", &english());
    assert_eq!((found[0].section, found[0].heading, found[0].hits.len()), (5, None, 1));
}

#[test]
fn enter_opens_the_first_focusable_result() {
    let all = sections();
    assert_eq!(first_target(&search(&all, "thumb", &english()), &all), Some((0, 1)));
    assert_eq!(first_target(&search(&all, "effective", &english()), &all), Some((3, 3)));
    assert_eq!(first_target(&search(&all, "zzz", &english()), &all), None);
}
