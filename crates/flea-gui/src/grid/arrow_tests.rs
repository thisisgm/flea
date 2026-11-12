use super::{arrow_with_wrap as arrow, Arrow};

#[test]
fn horizontal_row_wrap_is_opt_in_and_stays_inside_the_listing() {
    assert_eq!(arrow(3, Some(2), 8, Arrow::Right, false), None);
    assert_eq!(arrow(3, Some(2), 8, Arrow::Right, true), Some(3));
    assert_eq!(arrow(3, Some(3), 8, Arrow::Left, true), Some(2));
    assert_eq!(arrow(3, Some(0), 8, Arrow::Left, true), None);
    assert_eq!(arrow(3, Some(7), 8, Arrow::Right, true), None);
    assert_eq!(arrow(3, Some(6), 7, Arrow::Left, true), Some(5));
    assert_eq!(arrow(3, Some(6), 7, Arrow::Right, true), None);
    assert_eq!(arrow(3, Some(2), 8, Arrow::Down, true), Some(5));
}

#[test]
fn issue_434_one_column_short_rows_empty_and_invalid_cursors_are_bounded() {
    for (columns, at, total, left, right) in [
        (1, 0, 3, None, Some(1)),
        (1, 1, 3, Some(0), Some(2)),
        (1, 2, 3, Some(1), None),
        (0, 1, 3, Some(0), Some(2)),
        (3, 5, 7, Some(4), Some(6)),
        (3, 6, 7, Some(5), None),
        (3, 6, 8, Some(5), Some(7)),
        (3, 7, 8, Some(6), None),
        (3, 0, 1, None, None),
        (3, 0, 0, None, None),
        (3, 8, 8, None, None),
        (3, usize::MAX, 8, None, None),
        (3, usize::MAX - 1, usize::MAX, Some(usize::MAX - 2), None),
    ] {
        assert_eq!(arrow(columns, Some(at), total, Arrow::Left, true), left);
        assert_eq!(arrow(columns, Some(at), total, Arrow::Right, true), right);
    }
    for action in [Arrow::Left, Arrow::Right, Arrow::Up, Arrow::Down] {
        assert_eq!(arrow(3, None, 8, action, true), None);
        assert_eq!(arrow(3, Some(8), 8, action, true), None);
    }
}
