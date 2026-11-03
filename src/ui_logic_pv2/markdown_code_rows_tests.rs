use super::*;

// Every character ten pixels wide, the ellipsis too.
struct Mono;

impl Measure for Mono {
    fn width(&mut self, text: &str, _face: Face, _px: f32) -> f32 {
        text.chars().count() as f32 * 10.0
    }

    fn extent(&mut self, _face: Face, _px: f32) -> (f32, f32) {
        (8.0, 2.0)
    }
}

fn texts(lines: &[Vec<Piece>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.iter().map(|piece| piece.text.as_str()).collect())
        .collect()
}

fn runs(parts: &[(i64, &str)]) -> Vec<(i64, String)> {
    parts
        .iter()
        .map(|(role, text)| (*role, text.to_string()))
        .collect()
}

#[test]
fn gutter_holds_two_digits_at_least_and_one_more() {
    assert_eq!(gutter_columns(0), 3);
    assert_eq!(gutter_columns(9), 3);
    assert_eq!(gutter_columns(99), 3);
    assert_eq!(gutter_columns(100), 4);
    assert_eq!(gutter_columns(12_345), 6);
}

#[test]
fn plain_rows_drop_line_ends_and_a_final_empty_line() {
    let text = "a\r\nb\n\nc\n";
    let starts = vec![0, 3, 5, 6];
    assert_eq!(row_count(text, &starts), 4);
    assert_eq!(plain_row(text, &starts, 1), Some(runs(&[(0, "a")])));
    assert_eq!(plain_row(text, &starts, 2), Some(runs(&[(0, "b")])));
    assert_eq!(plain_row(text, &starts, 3), Some(runs(&[(0, "")])));
    assert_eq!(plain_row(text, &starts, 4), Some(runs(&[(0, "c")])));
    assert_eq!(plain_row(text, &starts, 5), None);
    assert_eq!(plain_row(text, &starts, 0), None);
    assert_eq!(row_count("", &[0]), 0);
}

#[test]
fn a_line_that_fits_keeps_its_runs_and_their_places() {
    let lines = row_lines(
        &runs(&[(5, "fn"), (0, " x")]),
        100.0,
        false,
        14.0,
        &mut Mono,
    );
    assert_eq!(
        lines,
        vec![vec![
            Piece {
                start: 0,
                len: 2,
                x: 0.0,
                role: 5,
                text: "fn".into()
            },
            Piece {
                start: 2,
                len: 2,
                x: 20.0,
                role: 0,
                text: " x".into()
            }
        ]]
    );
    assert_eq!(
        row_lines(&[], 100.0, true, 14.0, &mut Mono),
        vec![Vec::<Piece>::new()]
    );
}

#[test]
fn an_unwrapped_line_ends_in_an_ellipsis_in_the_ink_it_is_cut_in() {
    let lines = row_lines(
        &runs(&[(1, "abc"), (2, "defgh")]),
        50.0,
        false,
        14.0,
        &mut Mono,
    );
    assert_eq!(texts(&lines), vec!["abcd\u{2026}"]);
    assert_eq!(
        lines[0][1],
        Piece {
            start: 3,
            len: 1,
            x: 30.0,
            role: 2,
            text: "d\u{2026}".into()
        }
    );
    let cut_at_run = row_lines(
        &runs(&[(1, "abcd"), (2, "efgh")]),
        50.0,
        false,
        14.0,
        &mut Mono,
    );
    assert_eq!(
        cut_at_run[0],
        vec![
            Piece {
                start: 0,
                len: 4,
                x: 0.0,
                role: 1,
                text: "abcd".into()
            },
            Piece {
                start: 4,
                len: 0,
                x: 40.0,
                role: 2,
                text: "\u{2026}".into()
            }
        ]
    );
}

#[test]
fn a_wrapped_line_breaks_after_a_lone_space_and_anywhere_in_a_long_word() {
    let lines = row_lines(&runs(&[(0, "ab cd ef")]), 50.0, true, 14.0, &mut Mono);
    assert_eq!(texts(&lines), vec!["ab cd", "ef"]);
    let long = row_lines(&runs(&[(0, "abcdefghij kl")]), 40.0, true, 14.0, &mut Mono);
    assert_eq!(texts(&long), vec!["abcd", "efgh", "ij", "kl"]);
    // Two spaces hold together, so the words either side of them never part there.
    let held = row_lines(&runs(&[(0, "ab  cd")]), 40.0, true, 14.0, &mut Mono);
    assert_eq!(texts(&held), vec!["ab  ", "cd"]);
}

#[test]
fn gm_prevsel_source_spans_retain_offsets_after_a_hidden_wrap_space_and_elision() {
    let lines = row_lines(&runs(&[(0, "ab cd ef")]), 50., true, 14., &mut Mono);
    assert_eq!(lines[1][0].start, 6);
    assert_eq!(lines[1][0].len, 2);
    let cut = row_lines(&runs(&[(0, "abcdefgh")]), 50., false, 14., &mut Mono);
    assert_eq!(cut[0][0].len, 4);
    assert_eq!(cut[0][0].text, "abcd\u{2026}");
}
