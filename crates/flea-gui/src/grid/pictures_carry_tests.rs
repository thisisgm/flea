//! Query generations reuse completed source pictures without decoding returning rows again.
use super::*;

#[gpui::test]
fn fx477_search_backspace_reuses_recent_pictures_without_new_requests_or_decodes(
    cx: &mut TestAppContext,
) {
    for view in [View::Grid, View::List] {
        let (handle, mut lines) = images(cx);
        handle
            .update(cx, |shell, window, cx| {
                shell.set_view(view, window, cx);
                shell.search_action("search", window, cx);
            })
            .unwrap();
        search(cx, handle, "photo", &["photo-18.png", "sub/photo-31.png"]);
        first_settle(cx, handle, &mut lines);
        for row in 0..2 {
            deliver(cx, handle, &thumbed(row, PICTURE));
        }
        cx.run_until_parked();
        step(cx, handle);
        search(cx, handle, "31", &["sub/photo-31.png"]);
        step(cx, handle);
        settle(cx, handle, 120);
        drained(&mut lines);
        search(cx, handle, "photo", &["photo-18.png", "sub/photo-31.png"]);
        step(cx, handle);
        read(cx, handle, |shell| {
            let host = shell.panes[0].as_ref().unwrap();
            let pictures = &host.list.borrow().pictures;
            assert!(
                (0..2).all(|row| pictures.ready(&host.pane.held, row)),
                "backspace reuses both completed pictures"
            );
            assert_eq!(pictures.counters().decoded, 2);
        });
        settle(cx, handle, 120);
        assert!(rows(&drained(&mut lines), "thumb").is_empty());
    }
}
