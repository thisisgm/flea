//! The owning HTML frame and active viewport are independent IPC observations.
use super::*;
use crate::ipc::rows::rect_of;
use flea::ui_logic_doc_preview::html::View;

fn mounted(shell: &Entity<Shell>, cx: &VisualTestContext, column: bool) -> Entity<HtmlView> {
    shell.read_with(cx, |shell, cx| {
        if column {
            let preview = shell.selection_preview(0).unwrap().read(cx);
            let frame = preview.pending.as_ref().or(preview.drawn.as_ref()).unwrap();
            let Mounted::Doc(Doc::Html(view)) = &frame.mounted else {
                panic!("HTML column mounted")
            };
            view.clone()
        } else {
            let look = shell.quick_look.as_ref().unwrap().view.read(cx);
            let LookDoc::Html(view) = &look.subject().unwrap().body.doc().unwrap().view else {
                panic!("HTML Quick Look mounted")
            };
            view.clone()
        }
    })
}

fn open(cx: &mut TestAppContext, column: bool) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = test_shell::open_root_with(
        cx,
        if column {
            r#""keys":"default","view":"columns","preview":{"column":true}"#
        } else {
            r#""keys":"default""#
        },
        None,
    );
    test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("page.html", "text-html", "HTML", 0o100644)],
        1,
    );
    if column {
        super::super::previewtypes_tests::answer(
            &shell,
            cx,
            r#"{"t":"fsinfo","path":"/files","fs":"tmpfs","dev":1,"total":100,"free":90,"class":"local"}"#,
        );
        settle(cx);
        let lines = test_shell::sent();
        let meta = super::super::previewtypes_tests::ask(&lines, "meta");
        let token = super::super::previewtypes_tests::field(meta, "token");
        super::super::previewtypes_tests::answer(
            &shell,
            cx,
            &format!(r#"{{"t":"meta","token":{token},"path":"/files/page.html","lines":300}}"#),
        );
    } else {
        cx.simulate_keystrokes("space");
    }
    settle(cx);
    (shell, cx)
}

#[gpui::test]
fn pc2_contract_html_frame_is_own_outer_painted_border(cx: &mut TestAppContext) {
    for column in [false, true] {
        let (shell, cx) = open(cx, column);
        let view = mounted(&shell, cx, column);
        picture(&view, cx, 2000);
        for theme in [
            crate::palette::Theme::default(),
            crate::presentation::frame_gate::theme(),
        ] {
            cx.update(|_, cx| cx.set_global(theme));
            for width in [1100., 820.] {
                cx.simulate_resize(gpui::size(px(width), px(700.)));
                settle(cx);
                let outer = cx
                    .debug_bounds(if column {
                        "column-preview-frame"
                    } else {
                        "pv1-card"
                    })
                    .expect("the owning frame paints");
                let (scale, quads) =
                    cx.update(|window, _| (window.scale_factor(), window.painted_quads()));
                assert!(
                    quads
                        .iter()
                        .any(|quad| quad.bounds == outer.scale(scale)
                            && quad.border_widths.top.0 > 0.)
                );
                let expected = rect_of(
                    outer.origin.x.into(),
                    outer.origin.y.into(),
                    outer.size.width.into(),
                    outer.size.height.into(),
                );
                assert_eq!(
                    state(&shell, cx, column)
                        .get("frame")
                        .and_then(Json::as_str),
                    Some(expected.as_str()),
                    "column={column}, width={width}"
                );
            }
        }
    }
}

#[gpui::test]
fn pc2_contract_html_source_uses_active_scroll_viewport_and_content(cx: &mut TestAppContext) {
    for column in [false, true] {
        let (shell, cx) = open(cx, column);
        let view = mounted(&shell, cx, column);
        picture(&view, cx, 2000);
        let bounds = cx.read(|cx| view.read(cx).bounds.unwrap());
        wheel(cx, bounds.center());
        let rendered = state(&shell, cx, column);
        assert!(rendered.get("y").and_then(Json::as_f64).unwrap() > 0.);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let generation = view.set_path("/files/page.html", 600, true, cx);
                view.set_source_text(generation, Some("single source line".into()), false, cx);
            })
        });
        // Set_path resets Rendered; move it again before switching, as the retained native scene does.
        picture(&view, cx, 2000);
        wheel(cx, bounds.center());
        let rendered = state(&shell, cx, column);
        if column {
            cx.update(|_, cx| view.update(cx, |view, cx| view.set_view(View::Source, cx)));
        } else {
            cx.simulate_keystrokes("r");
        }
        settle(cx);
        let lines = test_shell::sent();
        let asked = super::super::previewtypes_tests::ask(&lines, "code");
        super::super::previewtypes_tests::answer(
            &shell,
            cx,
            &format!(
                r#"{{"t":"code","token":{},"slot":"{}","state":"plain","first":1,"total":1,"lines":[[[0,"single source line"]]]}}"#,
                super::super::previewtypes_tests::field(asked, "token"),
                if column { "column" } else { "look" }
            ),
        );
        settle(cx);
        let source = state(&shell, cx, column);
        let (height, row) = cx.read(|cx| {
            let code = view.read(cx).source.as_ref().unwrap().read(cx);
            (code.frame.1, code.metrics.row)
        });
        assert_eq!(source.get("view").and_then(Json::as_str), Some("source"));
        assert_eq!(source.get("y").and_then(Json::as_f64), Some(0.));
        assert_eq!(
            source.get("viewportHeight").and_then(Json::as_f64),
            Some(height as f64)
        );
        assert_eq!(
            source.get("contentHeight").and_then(Json::as_f64),
            Some(row as f64)
        );
        assert_ne!(source.get("viewportHeight"), rendered.get("viewportHeight"));
        assert_ne!(source.get("contentHeight"), rendered.get("contentHeight"));
        cx.update(|_, cx| view.update(cx, |view, cx| view.set_view(View::Rendered, cx)));
        settle(cx);
        let restored = state(&shell, cx, column);
        for key in ["y", "viewportHeight", "contentHeight"] {
            assert_eq!(
                restored.get(key),
                rendered.get(key),
                "Rendered retains its own {key}"
            );
        }
    }
}
