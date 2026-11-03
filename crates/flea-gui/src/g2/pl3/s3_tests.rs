//! S3 Trash faults and the held first browse, through the window's real event path.
use super::join_tests::{answer, call, draw, opened, root, shell};
use crate::{backend::Backend, list::bench::real_cx};

#[test]
fn s3_ey578_garbled_trash_refresh_reports_failure_without_dropping_rows() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| pane.action("reload", false, cx));
        shell.flush_trash(window, cx);
        let request = pane.read(cx).view().request_id;
        shell.receive(
            0,
            Backend::test_wire_line("garbled Trash reply"),
            window,
            cx,
        );
        assert_eq!(pane.read(cx).view().request_id, request);
        assert_eq!(pane.read(cx).view().total, 2);
        assert!(
            pane.read(cx).view().busy(),
            "the live browse can still answer"
        );
    });
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "statusError", &[]), "true");
    assert_eq!(
        call(&mut cx, window, "statusPrimary", &[]),
        "That action could not be completed; try again."
    );
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":2,"start":0,"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity"},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity"}]"#,
    );
    answer(
        &mut cx,
        window,
        "summary",
        r#", "bytes":11,"partial":false"#,
    );
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "statusError", &[]), "true");
}

#[test]
fn s3_ey579_held_first_trash_browse_keeps_its_head_button_and_headers() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        assert!(shell.trash_action("openTrash", window, cx));
        let host = shell.trash.as_mut().unwrap();
        host.presentation.epoch =
            cx.background_executor().now() - std::time::Duration::from_secs(2);
        shell.flush_trash(window, cx);
    });
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert!(pane.presentation_loading);
        assert!(pane.view().busy());
        assert_eq!(pane.view().total, 0);
        assert!(!pane.empty_available());
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), &cx);
    let back = visual
        .debug_bounds("trash-back")
        .expect("Trash back mark stays");
    let up = visual
        .debug_bounds("trash-up")
        .expect("Trash up mark stays");
    let button = visual
        .debug_bounds("trash-empty")
        .expect("Empty Trash stays");
    let header = visual
        .debug_bounds("trash-header")
        .expect("column headers stay");
    let line = visual
        .debug_bounds("trash-empty-line")
        .expect("Reading Trash stays");
    let area = visual.debug_bounds("trash-view").unwrap();
    assert_eq!(back.origin.y, up.origin.y);
    assert!(button.size.width > gpui::px(0.));
    assert!(header.origin.y > back.origin.y);
    assert_eq!(line.center(), area.center());
}
