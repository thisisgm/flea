//! Page repeats keep their stride while the cursor changes the neighbouring track.
use super::{columns_tests::host, scrollkeys_tests::mounted, startup::View};
use crate::pane::Held;
use gpui::VisualTestContext;
use std::rc::Rc;

#[test]
fn scrollkeys_columns_page_repeats_keep_half_pages_before_the_next_frame() {
    let mut cx = host::context();
    let handle = mounted(&mut cx, View::Columns);
    host::change(&mut cx, handle, |shell, _, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        let mut rows = pane.held.rows.clone();
        rows[10].dir = true;
        rows[10].icon = "folder".into();
        pane.held = Rc::new(Held {
            listing: pane.held.listing,
            total: pane.held.total,
            start: 0,
            rows,
        });
    });
    host::frame(&mut cx, handle);
    VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("pagedown pagedown");
    host::change(&mut cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 20);
    });
}
