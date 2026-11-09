use super::*;

fn grid_of(rows: Vec<Vec<&str>>) -> Vec<Vec<String>> {
    rows.into_iter().map(|row| row.into_iter().map(String::from).collect()).collect()
}

#[test]
fn csv_is_number() {
    assert!(is_number("20000"), "a whole number is numeric");
    assert!(is_number("-1.5"), "a negative decimal is numeric");
    assert!(is_number("1e3"), "an exponent is numeric");
    assert!(is_number("+.5"), "a leading plus and a bare fraction are numeric");
    assert!(is_number(" 7 "), "surrounding spaces do not stop a number");
    assert!(!is_number(""), "an empty cell is not numeric");
    assert!(!is_number("   "), "a blank cell is not numeric");
    assert!(!is_number("abc"), "a word is not numeric");
    assert!(is_number("0x1F"), "a hex literal parses as a finite number");
    assert!(!is_number("1,000"), "a thousands separator is not numeric");
    assert!(!is_number("Infinity"), "Infinity is not numeric");
    assert!(!is_number("1e309"), "an overflowing exponent is not finite");
}

#[test]
fn csv_numeric_columns() {
    let rows = grid_of(vec![vec!["a", "1", "x"], vec!["b", "", "2"], vec!["c", "-3.5", ""]]);
    assert_eq!(numeric_columns(&rows, 3), vec![false, true, true], "a column of numbers with blanks is right-aligned");
    assert_eq!(
        numeric_columns(&grid_of(vec![vec!["a", ""], vec!["b", ""]]), 2),
        vec![false, false],
        "a column with an empty body is left-aligned"
    );
    assert_eq!(
        numeric_columns(&grid_of(vec![vec!["files"], vec!["12"]]), 1),
        vec![true],
        "the header row is not tested"
    );
    assert_eq!(
        numeric_columns(&grid_of(vec![vec!["1"], vec!["2"], vec!["x"]]), 1),
        vec![false],
        "one word anywhere in a column left-aligns it"
    );
}
