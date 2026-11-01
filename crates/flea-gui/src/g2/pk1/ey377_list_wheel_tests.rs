//! EY-377: the wheel over the Open File List scrolls its rows and leaves the cursor, as PickerList.qml's ListView does.
use super::ey313_tests::draw;
use super::pointer_tests::{notch, seed};
use super::tests::fixture::fixture;
use gpui::TestAppContext;

#[gpui::test]
fn ey377_wheel_scrolls_the_list_and_asks_for_the_rows_it_shows(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    seed(&mut f);
    draw(&mut f);
    let asked = f.listing.sent().len();
    for _ in 0..40 {
        notch(&mut f, -3., false);
    }
    let (scroll, cursor) = f.update(|picker, cx| {
        let view = picker.browse.listing.read(cx);
        (view.scroll_y(), view.listing().cursor())
    });
    assert!(scroll > 0., "the wheel moved the list");
    assert_eq!(cursor, 0, "the wheel never moves the cursor");
    assert!(
        f.listing.sent()[asked..]
            .iter()
            .any(|line| line.contains(r#""c":"window""#)),
        "rows past the held window are asked for"
    );
    for _ in 0..80 {
        notch(&mut f, 3., false);
    }
    assert_eq!(
        f.update(|picker, cx| picker.browse.listing.read(cx).scroll_y()),
        0.,
        "the wheel stops at the first row"
    );
}
