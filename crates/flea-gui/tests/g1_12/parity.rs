//! The mounted crumb surface obeys the same blocking input state as the toolbar.
use super::{inputs, view::bar};
use flea_gui::path::{
    paint::Size,
    suggestions::Unranked,
    view::{Inputs, Out},
};
use gpui::TestAppContext;

#[gpui::test]
fn pb_path_blocked_crumbs_take_neither_parent_taps_nor_leaf_double_taps(cx: &mut TestAppContext) {
    let path = "/home/gm/Work/claude";
    let mut bar = bar(
        cx,
        Box::new(Unranked),
        Inputs {
            blocked: true,
            ..inputs(path)
        },
    );
    let cell = bar.advance(Size::Caption);
    bar.press(cell * 4.5, 13., 1);
    bar.press(cell * 9., 13., 1);
    bar.press(cell * 9., 13., 2);
    assert!(bar.take().is_empty());
    assert!(!bar.read(|view| view.editing()));
    bar.sync(inputs(path));
    bar.press(cell * 4.5, 13., 1);
    assert_eq!(
        bar.take(),
        [Out::Open {
            side: 0,
            path: "/home/gm/Work".into()
        }]
    );
}
