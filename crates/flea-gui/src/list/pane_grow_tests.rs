//! The refused rename's band: the edited row grows, the rows below it move down, and a press lands where the row is drawn.
use super::Grow;

#[test]
fn no_band_keeps_the_fixed_row_arithmetic() {
    let grow = Grow::default();
    assert_eq!(grow.top(3, 31.), 93.);
    assert_eq!(grow.height(3, 31.), 31.);
    assert_eq!(grow.position_at(93., 31.), 3);
    assert_eq!(grow.position_at(92.9, 31.), 2);
    assert_eq!(grow.content(4, 31.), 124.);
    assert_eq!(grow.span(0., 100., 31., 10), (0, 4));
}

#[test]
fn a_band_moves_only_the_rows_below_its_edited_row() {
    let grow = Grow { at: 2, band: 25. };
    assert_eq!(grow.top(1, 31.), 31.);
    assert_eq!(grow.top(2, 31.), 62., "the edited row keeps its top");
    assert_eq!(
        grow.top(3, 31.),
        118.,
        "the row below moves down by the band"
    );
    assert_eq!(grow.height(2, 31.), 56.);
    assert_eq!(grow.height(3, 31.), 31.);
    assert_eq!(grow.content(4, 31.), 149.);
}

#[test]
fn a_press_in_the_band_lands_on_the_edited_row() {
    let grow = Grow { at: 2, band: 25. };
    assert_eq!(grow.position_at(61., 31.), 1);
    assert_eq!(grow.position_at(62., 31.), 2);
    assert_eq!(grow.position_at(100., 31.), 2, "inside the band");
    assert_eq!(grow.position_at(117.9, 31.), 2);
    assert_eq!(grow.position_at(118., 31.), 3);
    assert_eq!(grow.position_at(149., 31.), 4);
    assert_eq!(grow.span(0., 100., 31., 10), (0, 3));
}

#[test]
fn a_row_is_marked_only_where_the_view_draws_it() {
    let shown = [9usize, 5, 7];
    let filtered = Some(&shown[..]);
    assert_eq!(
        Grow::of_row(Some(5), 25., filtered),
        Grow { at: 1, band: 25. }
    );
    assert_eq!(Grow::of_row(Some(5), 25., None), Grow { at: 5, band: 25. });
    assert_eq!(
        Grow::of_row(Some(8), 25., filtered),
        Grow::default(),
        "a filtered row has no band"
    );
    assert_eq!(
        Grow::of_row(Some(5), 0., None),
        Grow::default(),
        "no refusal, no band"
    );
    assert_eq!(Grow::of_row(None, 25., None), Grow::default());
}
