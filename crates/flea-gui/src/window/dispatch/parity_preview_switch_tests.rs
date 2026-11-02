//! EY-545 after a drawn frame: once the hold expires the column presents the pending row, not the old one.
use super::{
    parity_preview_column_tests::mark_shown,
    previewtypes_kinds_tests::Root,
    previewtypes_tests::{answer, ask, field, file, local, meta, open_at, pump, thumbed},
    test_shell::land,
};
use crate::window::u07_tests::read;
use gpui::TestAppContext;

#[gpui::test]
fn sol_preview_remainder_ey545_hold_expiry_presents_the_pending_row(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("ey545-switch");
    let dir = root.0.to_string_lossy().into_owned();
    let page = root.0.join("page.png");
    image::RgbImage::from_pixel(32, 24, image::Rgb([255; 3]))
        .save(&page)
        .unwrap();
    std::fs::write(
        root.0.join("a.jpg"),
        include_bytes!("../../../tests/fixtures/ey545-image-jpg.jpg"),
    )
    .unwrap();
    std::fs::write(root.0.join("b.html"), "<p>sweep</p>").unwrap();
    let (shell, cx) = open_at(cx, &dir, Some(14.));
    let mut rows = vec![
        file("a.jpg", "image-jpeg", "JPEG image", 0o100644),
        file("b.html", "text-html", "HTML document", 0o100644),
    ];
    for row in &mut rows {
        row.size = 4800;
    }
    land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":320,"h":240"#);
    thumbed(&shell, cx, &lines, 0, "");
    pump(cx);
    assert_ne!(
        read(&shell, cx, "previewColumnState", &[]),
        "loading",
        "a.jpg is drawn"
    );

    cx.simulate_keystrokes("down");
    let mut lines = pump(cx);
    let (drawn, pending, holding) = shell.read_with(cx, |shell, cx| {
        let view = shell.selection_preview(0).unwrap().read(cx);
        let holding = view.port.0.borrow().holding;
        (view.drawn.is_some(), view.pending.is_some(), holding)
    });
    assert!(
        drawn && pending && !holding,
        "a.jpg is retained, b.html pends, the hold expired"
    );
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "loading");
    for selector in [
        "column-preview-frame",
        "column-preview-waiting",
        "column-preview-name",
        "column-preview-facts",
        "column-preview-loading-mark",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(
        mark_shown(&shell, cx),
        "the Omi plays over the pending frame"
    );
    assert_eq!(
        read(&shell, cx, "previewFacts", &[]),
        "Kind=HTML document|Size=4.8 kB|State=loading",
        "the facts are the pending row's, never the retained JPEG's"
    );

    meta(&shell, cx, &lines, "");
    lines.extend(pump(cx));
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"html","token":{},"slot":"column","state":"ready","file":"{}","total":1,"width":32,"title":""}}"#,
            field(ask(&lines, "html"), "token"),
            page.display()
        ),
    );
    pump(cx);
    assert!(cx.debug_bounds("column-preview-waiting").is_none());
    assert!(!mark_shown(&shell, cx), "the landed page stops the Omi");
    assert!(!read(&shell, cx, "previewFacts", &[]).contains("State=loading"));
}
