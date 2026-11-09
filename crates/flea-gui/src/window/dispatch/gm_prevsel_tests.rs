//! Preview selection uses the production Columns view and Shell key delivery.
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::{
    g2::doc_preview::code::CodeView,
    state::Shell,
    window::selection_preview::{docs::Doc, Mounted},
};
use gpui::{
    point, px, ClipboardItem, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext,
};

type Fixture<'a> = (
    Entity<Shell>,
    &'a mut VisualTestContext,
    Entity<CodeView>,
    Point<Pixels>,
    Point<Pixels>,
);

/// A Columns code preview of two Unicode lines, with a press point on the first and a drag point past the second.
fn code_preview<'a>(cx: &'a mut TestAppContext, numbered: bool) -> Fixture<'a> {
    let (shell, cx) = open(cx, "/files");
    let (name, icon) = if numbered {
        ("main.rs", "text-x-rust")
    } else {
        ("notes.txt", "text-x-generic")
    };
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file(name, icon, "Text", 0o100644)],
        1,
    );
    let asks = local(&shell, cx, "/files");
    let code = ask(&asks, "code").to_owned();
    meta(&shell, cx, &asks, r#","lines":200"#);
    let mut body = vec![
        r#"[5,"héllo",0," world"]"#.to_owned(),
        r#"[0,"第二 line"]"#.to_owned(),
    ];
    body.extend((2..200).map(|n| format!(r#"[0,"line {n}"]"#)));
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":{},"total":200,"lines":[{}]}}"#,
            field(&code, "token"),
            field(&code, "first"),
            body.join(",")
        ),
    );
    pump(cx);
    let (view, bounds) = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap().read(cx);
        let Some(Mounted::Doc(Doc::Code(code))) = preview.drawn.as_ref().map(|f| &f.mounted) else {
            panic!("code drawn")
        };
        (code.clone(), preview.frame_rect.borrow().unwrap())
    });
    let (start, end) = cx.update(|window, cx| {
        let code = view.read(cx);
        let id = window.text_system().resolve_font(&code.typeface.font);
        let advance = window
            .text_system()
            .advance(id, code.typeface.body, '0')
            .unwrap()
            .width;
        let x = bounds.origin.x + px(1. + code.text_left(advance));
        let y = bounds.origin.y + px(1. + code.metrics.top + code.metrics.row / 2.);
        cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
        (
            point(x, y),
            point(bounds.right() - px(30.), y + px(code.metrics.row)),
        )
    });
    (shell, cx, view, start, end)
}

/// A test root that carries its marker is removed on pass and on panic.
struct Root(std::path::PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        if self.0.is_absolute() && self.0.join(".flea-test-root").is_file() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn drag(cx: &mut VisualTestContext, start: Point<Pixels>, end: Point<Pixels>) {
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn gm_prevsel_drag_copies_unicode_lines_without_scrolling_or_taking_focus(cx: &mut TestAppContext) {
    for numbered in [false, true] {
        let (shell, cx, view, start, end) = code_preview(cx, numbered);
        drag(cx, start, end);
        cx.simulate_keystrokes("ctrl-c");
        pump(cx);
        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(
            copied.as_deref(),
            Some("héllo world\n第二 line"),
            "drag selects displayed text, not files"
        );
        view.read_with(cx, |view, _| assert_eq!((view.top, view.offset), (0, 0.)));
        assert!(
            crate::window::u07_tests::read(&shell, cx, "keyDeliveryState", &[])
                .contains("\"listFocus\":true")
        );
        cx.simulate_keystrokes("escape");
        pump(cx);
        view.read_with(cx, |view, _| assert!(view.selection.range().is_none()));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_down(
            end,
            MouseButton::Left,
            Modifiers {
                shift: true,
                ..Modifiers::none()
            },
        );
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text()),
            Some("héllo world\n第二 line".into())
        );
        cx.simulate_event(gpui::MouseDownEvent {
            position: start + point(px(4.), px(0.)),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text()),
            Some("héllo".into())
        );
        cx.simulate_mouse_down(
            point(px(5.), px(690.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(px(5.), px(690.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        view.read_with(cx, |view, _| assert!(view.selection.range().is_none()));
        let _ = pump(cx);
        cx.simulate_keystrokes("ctrl-c");
        assert!(
            pump(cx)
                .iter()
                .any(|line| line.contains("\"c\":\"paths\"") && line.contains("\"rows\":[0]")),
            "without a preview range Copy still asks the file clipboard"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn gm_prevsel_quick_look_escape_clears_before_closing_and_copy_keeps_the_key_table(
    cx: &mut TestAppContext,
) {
    use super::preview_tests as preview;
    use crate::g2::quick_look_overlay::doc::View;
    let (shell, cx, mut requests) = preview::opened(cx);
    super::test_shell::land(&shell, cx, "/files", preview::rows(), 1);
    super::test_shell::set_cursor(&shell, cx, 1);
    preview::press(cx, "space");
    assert!(shell.read_with(cx, |shell, cx| shell.quick_look_open(cx)));
    let asked = preview::sent(&mut requests, "code");
    assert_eq!(asked.len(), 1, "Quick Look asks its text lines");
    preview::answer(
        &shell,
        cx,
        preview::code_reply(preview::token(&asked[0]), &["first", "second"], 2),
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    preview::draw(cx);
    let code = shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        let View::Code(code) = &look.subject().unwrap().body.doc().unwrap().view else {
            panic!("text document")
        };
        code.clone()
    });
    let (start, end) = code.read_with(cx, |code, _| {
        let first = code.selection.spans.first().unwrap();
        let last = code.selection.spans.last().unwrap();
        (
            point(first.bounds.left(), first.bounds.center().y),
            point(last.bounds.right(), last.bounds.center().y),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().unwrap().text()),
        Some("first\nsecond".into())
    );
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |shell, cx| shell.quick_look_open(cx)));
    code.read_with(cx, |code, _| assert!(code.selection.range().is_none()));
    let _ = preview::drain(&mut requests);
    cx.simulate_keystrokes("ctrl-c");
    assert!(
        preview::sent(&mut requests, "paths").is_empty(),
        "without a range Quick Look leaves Copy to its key table, which binds none there"
    );
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    code.update(cx, |code, cx| code.set_read_failed(cx));
    code.read_with(cx, |code, _| {
        assert!(
            code.selection.range().is_none(),
            "a failed preview drops its range"
        )
    });
    cx.simulate_keystrokes("escape");
    pump(cx);
    assert!(!shell.read_with(cx, |shell, cx| shell.quick_look_open(cx)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn gm_prevsel_markdown_drag_copies_through_the_shell_in_both_views(cx: &mut TestAppContext) {
    use crate::g2::doc_preview::markdown::View;
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join(".superpowers/GM-PREVSEL");
    std::fs::create_dir_all(&base).unwrap();
    let made = std::process::Command::new("mktemp")
        .args(["-d", base.join("markdown.XXXXXXXX").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(&base) && root != base);
    std::fs::write(root.join(".flea-test-root"), "").unwrap();
    let cleanup = Root(root.clone());
    std::fs::write(root.join("notes.md"), "# Notes\r\n\r\nfirst body\r\n").unwrap();
    let path = root.to_str().unwrap();
    let (shell, cx) = open(cx, path);
    cx.executor().allow_parking();
    let row = file("notes.md", "text-markdown", "Markdown", 0);
    super::test_shell::land(&shell, cx, path, vec![row], 1);
    let asks = local(&shell, cx, path);
    meta(&shell, cx, &asks, r#", "lines":3"#);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let markdown = loop {
        pump(cx);
        let ready = shell.read_with(cx, |shell, cx| {
            match &shell.selection_preview(0)?.read(cx).drawn.as_ref()?.mounted {
                Mounted::Doc(Doc::Markdown(view)) if view.read(cx).doc.content_ready() => {
                    Some(view.clone())
                }
                _ => None,
            }
        });
        if let Some(markdown) = ready {
            break markdown;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Markdown worker did not finish"
        );
        std::thread::yield_now();
    };
    for (view, want) in [
        (View::Rendered, "Notes\nfirst body"),
        (View::Source, "# Notes\r\n\r\nfirst body"),
    ] {
        markdown.update(cx, |markdown, cx| markdown.set_view(view, cx));
        let asked = pump(cx);
        if view == View::Source {
            let code = ask(&asked, "code");
            let slot = flea::jsondoc::parse(code).unwrap();
            let slot = slot
                .get("slot")
                .and_then(flea::jsondoc::Json::as_str)
                .unwrap();
            answer(
                &shell,
                cx,
                &format!(
                    r##"{{"t":"code","token":{},"slot":"{slot}","state":"plain","first":1,"total":3,"lines":[[0,"# Notes"],[0,""],[0,"first body"]]}}"##,
                    field(code, "token")
                ),
            );
            pump(cx);
        }
        if view == View::Rendered {
            cx.simulate_keystrokes("ctrl-l");
            assert_eq!(
                crate::window::u07_tests::read(&shell, cx, "pathBarOpen", &[]),
                "true"
            );
            pump(cx);
        }
        let (start, end) = markdown.read_with(cx, |markdown, _| {
            let origin = point(px(markdown.bounds.0), px(markdown.bounds.1));
            let spans = &markdown.selection.spans;
            let (first, last) = (spans.first().unwrap(), spans.last().unwrap());
            (
                origin + point(first.bounds.left(), first.bounds.center().y),
                origin + point(last.bounds.right(), last.bounds.center().y),
            )
        });
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into())));
        drag(cx, start, end);
        cx.simulate_keystrokes("ctrl-c");
        pump(cx);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some(want.into()),
            "{view:?}"
        );
        assert!(
            crate::window::u07_tests::read(&shell, cx, "keyDeliveryState", &[])
                .contains("\"listFocus\":true")
        );
        cx.simulate_keystrokes("escape");
        markdown.read_with(cx, |markdown, _| {
            assert!(markdown.selection.range().is_none())
        });
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    drop(cleanup);
    assert!(!root.exists(), "the test root is removed");
}

#[path = "gm_prevsel_tests/focus.rs"]
mod focus;
