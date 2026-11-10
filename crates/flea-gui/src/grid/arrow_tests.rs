use super::{arrow_with_wrap as arrow, Arrow};

#[test]
fn horizontal_row_wrap_is_opt_in_and_stays_inside_the_listing() {
    assert_eq!(arrow(3, Some(2), 8, Arrow::Right, false), None);
    assert_eq!(arrow(3, Some(2), 8, Arrow::Right, true), Some(3));
    assert_eq!(arrow(3, Some(2), 8, Arrow::ExtendRight, false), None);
    assert_eq!(arrow(3, Some(2), 8, Arrow::ExtendRight, true), Some(3));
    assert_eq!(arrow(3, Some(3), 8, Arrow::ExtendLeft, false), None);
    assert_eq!(arrow(3, Some(3), 8, Arrow::ExtendLeft, true), Some(2));
    assert_eq!(arrow(3, Some(3), 8, Arrow::Left, true), Some(2));
    assert_eq!(arrow(3, Some(0), 8, Arrow::Left, true), None);
    assert_eq!(arrow(3, Some(7), 8, Arrow::Right, true), None);
    assert_eq!(arrow(3, Some(0), 8, Arrow::ExtendLeft, true), None);
    assert_eq!(arrow(3, Some(7), 8, Arrow::ExtendRight, true), None);
    assert_eq!(arrow(3, Some(6), 7, Arrow::Left, true), Some(5));
    assert_eq!(arrow(3, Some(6), 7, Arrow::Right, true), None);
    assert_eq!(arrow(3, Some(6), 7, Arrow::ExtendRight, true), None);
    assert_eq!(arrow(3, Some(2), 8, Arrow::Down, true), Some(5));
}
