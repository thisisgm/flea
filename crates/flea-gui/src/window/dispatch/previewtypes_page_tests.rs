//! F and B page code by whole rows and rendered documents by their viewport height, through the production Shell.
use super::previewtypes_kinds_tests::Root;
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::{
    g2::doc_preview::{code::CodeView, html::HtmlView, markdown::MarkdownView},
    state::Shell,
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::ui_logic_doc_preview::html::View;
use gpui::{
    point, px, size, Entity, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
};
use std::time::{Duration, Instant};

fn doc<R>(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    read: impl FnOnce(&Doc, &gpui::App) -> R,
) -> R {
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

fn page_code_after_wheel(cx: &mut TestAppContext, wrap: bool) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file("long.rs", "text-x-rust", "Rust source code", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","lines":200"#);
    let asked = ask(&lines, "code").to_owned();
    let body = (0..200)
        .map(|n| {
            let text = if n == 0 && wrap {
                "x".repeat(3000)
            } else {
                format!("line {n}")
            };
            format!(r#"[0,"{text}"]"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":200,"lines":[{body}]}}"#,
            field(&asked, "token")
        ),
    );
    pump(cx);
    let view: Entity<CodeView> = doc(&shell, cx, |doc, _| match doc {
        Doc::Code(view) => view.clone(),
        _ => panic!("the code view is drawn"),
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.metrics.row = 20.;
            view.set_wrap(wrap, cx);
        })
    });
    pump(cx);
    let height = cx.read(|cx| view.read(cx).frame.1);
    cx.simulate_resize(size(px(1100.), px(700. + 505. - height)));
    pump(cx);
    assert_eq!(cx.read(|cx| view.read(cx).frame.1), 505.);
    let at = cx.debug_bounds("column-preview").unwrap().center();
    let position = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let view = view.read(cx);
            if wrap {
                assert_eq!(view.top, 0, "paging stays within the long wrapped line");
            }
            view.top as f32 * view.metrics.row + view.offset
        })
    };
    // One screen less a line: 505 px of 20 px rows pages by 24 rows, so the last line stays on top as context.
    let step = flea::ui_logic_doc_preview::code_view::page_step(505., 20.);
    assert_eq!(step, 480.);
    for (before, key, after) in [
        (19., "shift-f->F", step),
        (2. * step + 19., "shift-b->B", step),
    ] {
        cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(-19.))),
            ..Default::default()
        });
        pump(cx);
        assert_eq!(
            position(cx),
            before,
            "wheel scrolling reaches a partial row"
        );
        cx.simulate_keystrokes(key);
        pump(cx);
        assert_eq!(
            position(cx),
            after,
            "a page restores a whole first row under the top padding"
        );
        cx.simulate_keystrokes("shift-f->F");
        pump(cx);
        assert_eq!(
            position(cx),
            2. * step,
            "another page retains whole wrapped continuation rows"
        );
    }
}

#[gpui::test]
fn f_and_b_snap_code_after_pixel_wheel_scroll(cx: &mut TestAppContext) {
    page_code_after_wheel(cx, false);
}

#[gpui::test]
fn f_and_b_snap_wrapped_code_after_pixel_wheel_scroll(cx: &mut TestAppContext) {
    page_code_after_wheel(cx, true);
}

#[gpui::test]
fn f_and_b_page_rendered_markdown_by_its_viewport_height(cx: &mut TestAppContext) {
    // The markdown parse thread is real, so its wakes come from outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("pagemd");
    let dir = root.0.to_string_lossy().into_owned();
    let text = (0..400)
        .map(|n| format!("Paragraph {n}.\n\n"))
        .collect::<String>();
    std::fs::write(root.0.join("long.md"), text).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file(
        "long.md",
        "text-markdown",
        "Markdown document",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","lines":800"#);
    let deadline = Instant::now() + Duration::from_secs(10);
    // The swap draws the document only once the parse thread answers, so an undrawn column is still loading.
    let tall = |cx: &mut VisualTestContext| {
        let drawn = shell.read_with(cx, |shell, cx| {
            let preview = shell.selection_preview(0)?;
            match preview.read(cx).drawn.as_ref().map(|frame| &frame.mounted) {
                Some(Mounted::Doc(Doc::Markdown(view))) => Some(view.clone()),
                _ => None,
            }
        });
        drawn.is_some_and(|view| {
            cx.read(|cx| {
                view.read(cx).content_height() > 3. * view.read(cx).frame.1
                    && view.read(cx).frame.1 > 0.
            })
        })
    };
    while !tall(cx) {
        assert!(
            Instant::now() < deadline,
            "the long document parsed and laid out"
        );
        pump(cx);
        std::thread::sleep(Duration::from_millis(10));
    }
    let view = markdown(&shell, cx);
    let (top, height) = cx.read(|cx| (view.read(cx).scroll, view.read(cx).frame.1));
    assert_eq!(top, 0.);
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    assert_eq!(
        cx.read(|cx| view.read(cx).scroll),
        height,
        "F moves the rendered document by one viewport height"
    );
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(
        cx.read(|cx| view.read(cx).scroll),
        0.,
        "B pages back to the top"
    );
}

/// An HTML row whose page renders to a picture eight frames tall, drawn in the column.
fn tall_page<'a>(
    cx: &'a mut TestAppContext,
    root: &Root,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("page.html"), "<p>page</p>").unwrap();
    let picture = root.0.join("page.png");
    image::RgbaImage::from_pixel(100, 6000, image::Rgba([255; 4]))
        .save(&picture)
        .unwrap();
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
    assert!(
        top == 0. && height > 0. && page > 3. * height,
        "the page is decoded and laid out: {top} {height} {page}"
    );
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    assert_eq!(
        cx.read(|cx| view.read(cx).scroll_y),
        height,
        "F moves the page by one viewport height"
    );
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(
        cx.read(|cx| view.read(cx).scroll_y),
        0.,
        "B pages back to the top"
    );
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
    let body = (0..300)
        .map(|n| format!(r#"[0,"line {n}"]"#))
        .collect::<Vec<_>>()
        .join(",");
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
    let source = cx
        .read(|cx| view.read(cx).source.clone())
        .expect("Source built its code view");
    // Source lines are one row each here, so the scrolled distance is whole rows plus the offset.
    let moved = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let code = source.read(cx);
            (
                code.top as f32 * code.metrics.row + code.offset,
                code.frame.1,
                code.metrics.row,
                code.offset,
            )
        })
    };
    let (top, height, row, _) = moved(cx);
    assert!(
        top == 0. && height > 0.,
        "Source is laid out at its top: {top} {height}"
    );
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    let (paged, _, _, offset) = moved(cx);
    assert_eq!(
        paged,
        flea::ui_logic_doc_preview::code_view::page_step(height, row),
        "F pages Source by whole lines, so the first line draws whole"
    );
    assert_eq!(
        offset, 0.,
        "a page lands on a line boundary, never half a line"
    );
    assert_eq!(
        cx.read(|cx| view.read(cx).scroll_y),
        0.,
        "the rendered page under Source stays put"
    );
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(moved(cx).0, 0., "B pages Source back to the top");
}
