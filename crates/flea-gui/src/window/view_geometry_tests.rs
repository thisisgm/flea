//! Geometry readers require a painted List frame before reporting its dimensions.
use super::*;

#[test]
fn u02_geometry_readers_wait_for_the_first_list_frame() {
    let mut cx = context();
    let (backend, _requests) = Backend::test_channel();
    let window = cx.open_window(size(px(900.), px(600.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::states::Kind::Loading;
        shell
    });
    draw(&mut cx, window);
    let frames = window
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().list.borrow().drawn.frames
        })
        .unwrap();
    assert_eq!(frames, 0, "a List frame was painted");
    for function in ["visibleRows", "viewEndY"] {
        let reply = call(&mut cx, window, function, &[]);
        assert_ne!(
            reply.code, 0,
            "{function} answered {reply:?} before any List frame"
        );
    }
    assert_eq!(value(&mut cx, window, "drawnCount"), "0");
    assert_eq!(
        call(&mut cx, window, "rowAt", &["0"]),
        Reply::value("loading")
    );
}
