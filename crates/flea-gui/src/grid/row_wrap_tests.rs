use super::{arrow, Arrow};

// Issue 434: Right on the last cell of a row continues to the next row, and Left on the
// first cell continues to the previous row, only when the option is on. The listing ends hold.
#[test]
fn right_and_left_continue_onto_the_neighbouring_row_when_asked() {
    let hold = |view, step| arrow(3, Some(view), 7, step, false);
    assert_eq!(hold(2, Arrow::Right), None, "right holds at a row end");
    assert_eq!(hold(3, Arrow::Left), None, "left holds at a row start");
    let wrap = |view, step| arrow(3, Some(view), 7, step, true);
    assert_eq!(
        wrap(2, Arrow::Right),
        Some(3),
        "right continues to the next row"
    );
    assert_eq!(
        wrap(5, Arrow::Right),
        Some(6),
        "right continues onto a partial last row"
    );
    assert_eq!(
        wrap(3, Arrow::Left),
        Some(2),
        "left continues to the previous row"
    );
    assert_eq!(
        wrap(6, Arrow::Left),
        Some(5),
        "left from a short row continues back"
    );
    assert_eq!(wrap(6, Arrow::Right), None, "right holds on the last cell");
    assert_eq!(wrap(0, Arrow::Left), None, "left holds on the first cell");
    assert_eq!(
        wrap(1, Arrow::Right),
        Some(2),
        "a cell with a neighbour is unchanged"
    );
    assert_eq!(
        wrap(1, Arrow::Down),
        Some(4),
        "down still steps a whole row"
    );
    assert_eq!(
        wrap(4, Arrow::Down),
        None,
        "down still holds past the last row"
    );
}
