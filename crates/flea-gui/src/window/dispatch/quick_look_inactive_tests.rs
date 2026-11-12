use super::*;
use crate::g2::quick_look_overlay::doc::View;
use gpui::RequestFrameOptions;
use std::time::Duration;

#[gpui::test]
fn fx_inactive_pdf_opens_on_each_idle_frame_without_input(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![
            file("a.pdf", "application-pdf"),
            file("b.pdf", "application-pdf"),
        ],
        1,
    );
    for index in 0..2 {
        cx.executor().advance_clock(Duration::from_millis(
            flea::ui_logic_doc_preview::html::SETTLE_MS + 1,
        ));
        set_cursor(&shell, cx, index);
        shell.update_in(cx, |shell, window, cx| {
            assert!(shell.open_quick_look(0, window, cx))
        });
        draw(cx);
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("PDF mounted")
        };
        let View::Pdf(view) = doc.view else {
            panic!("PDF view mounted")
        };
        assert!(view.read_with(cx, |view, _| view.surface.opener.fast_armed));
        cx.simulate_frame_request(RequestFrameOptions::default());
        cx.run_until_parked();
        assert!(
            !view.read_with(cx, |view, _| view.surface.opener.fast_armed),
            "idle PDF must consume its next-frame opener"
        );
    }
    close(&shell, cx);
}
