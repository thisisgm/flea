//! F and B page the Markdown and HTML views by their viewport's height (PreviewScroll.qml pageBy), through the production Shell.
use super::previewtypes_kinds_tests::Root;
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::{
    g2::pv2::{html::HtmlView, markdown::MarkdownView},
    state::Shell,
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::ui_logic_pv2::html::View;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::{Duration, Instant};

fn doc<R>(shell: &Entity<Shell>, cx: &mut VisualTestContext, read: impl FnOnce(&Doc, &gpui::App) -> R) -> R {
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        match preview.read(cx).drawn.as_ref().map(|frame| &frame.mounted) {
            Some(Mounted::Doc(doc)) => read(doc, cx),
            _ => panic!("a document is drawn"),
        }
    })
}

fn markdown(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<MarkdownView> {
    doc(shell, cx, |doc, _| match doc {
        Doc::Markdown(view) => view.clone(),
        _ => panic!("the markdown view is drawn"),
    })
}

fn html(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<HtmlView> {
    doc(shell, cx, |doc, _| match doc {
        Doc::Html(view) => view.clone(),
        _ => panic!("the html view is drawn"),
    })
}

#[gpui::test]
fn f_and_b_page_rendered_markdown_by_its_viewport_height(cx: &mut TestAppContext) {
    // The markdown parse thread is real, so its wakes come from outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("pagemd");
    let dir = root.0.to_string_lossy().into_owned();
    let text = (0..400).map(|n| format!("Paragraph {n}.\n\n")).collect::<String>();
    std::fs::write(root.0.join("long.md"), text).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("long.md", "text-markdown", "Markdown document", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","lines":800"#);
    let deadline = Instant::now() + Duration::from_secs(10);
    let tall = |cx: &mut VisualTestContext| {
        let view = markdown(&shell, cx);
        cx.read(|cx| view.read(cx).content_height() > 3. * view.read(cx).frame.1 && view.read(cx).frame.1 > 0.)
    };
    while !tall(cx) {
        assert!(Instant::now() < deadline, "the long document parsed and laid out");
        pump(cx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let view = markdown(&shell, cx);
    let (top, height) = cx.read(|cx| (view.read(cx).scroll, view.read(cx).frame.1));
    assert_eq!(top, 0.);
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    assert_eq!(cx.read(|cx| view.read(cx).scroll), height, "F moves the rendered document by one viewport height");
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(cx.read(|cx| view.read(cx).scroll), 0., "B pages back to the top");
}

/// An HTML row whose page renders to a picture eight frames tall, drawn in the column.
fn tall_page<'a>(cx: &'a mut TestAppContext, root: &Root) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("page.html"), "<p>page</p>").unwrap();
    let picture = root.0.join("page.png");
    image::RgbaImage::from_pixel(100, 6000, image::Rgba([255; 4])).save(&picture).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("page.html", "text-html", "HTML document", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","lines":1"#);
    let lines = [lines, pump(cx)].concat();
    let asked = ask(&lines, "html");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"html","token":{},"slot":"column","state":"ready","file":"{}","total":1,"width":100,"title":""}}"#,
            field(asked, "token"),
            picture.display()
        ),
    );
    pump(cx);
    (shell, cx)
}

#[gpui::test]
fn f_and_b_page_a_rendered_html_page_by_its_viewport_height(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("pagehtml");
    let (shell, cx) = tall_page(cx, &root);
    let view = html(&shell, cx);
    let (top, height, page) = cx.read(|cx| {
        let view = view.read(cx);
        (view.scroll_y, view.frame.1, view.page_size().1 as f32)
    });
    assert!(top == 0. && height > 0. && page > 3. * height, "the page is decoded and laid out: {top} {height} {page}");
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    assert_eq!(cx.read(|cx| view.read(cx).scroll_y), height, "F moves the page by one viewport height");
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(cx.read(|cx| view.read(cx).scroll_y), 0., "B pages back to the top");
}

#[gpui::test]
fn f_and_b_page_html_source_by_its_viewport_height(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("pagesource");
    let (shell, cx) = tall_page(cx, &root);
    let view = html(&shell, cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.set_view(View::Source, cx)));
    let lines = pump(cx);
    let asked = ask(&lines, "code").to_owned();
    let body = (0..300).map(|n| format!(r#"[0,"line {n}"]"#)).collect::<Vec<_>>().join(",");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":{},"total":300,"lines":[{body}]}}"#,
            field(&asked, "token"),
            field(&asked, "first")
        ),
    );
    pump(cx);
    let source = cx.read(|cx| view.read(cx).source.clone()).expect("Source built its code view");
    // Source lines are one row each here, so the scrolled distance is whole rows plus the offset.
    let moved = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let code = source.read(cx);
            (code.top as f32 * code.metrics.row + code.offset, code.frame.1)
        })
    };
    let (top, height) = moved(cx);
    assert!(top == 0. && height > 0., "Source is laid out at its top: {top} {height}");
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    assert_eq!(moved(cx).0, height, "F moves Source by one viewport height");
    assert_eq!(cx.read(|cx| view.read(cx).scroll_y), 0., "the rendered page under Source stays put");
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(moved(cx).0, 0., "B pages Source back to the top");
}
