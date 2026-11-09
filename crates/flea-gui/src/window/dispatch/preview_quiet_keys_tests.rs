//! A focused column preview keeps Tab and takes keys it cannot use silently (SelectionPreview.qml); htmlState follows R (EY-331).
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::{state::Shell, window::u07_tests::read};
use flea::jsondoc::Json;
use gpui::{Entity, TestAppContext, VisualTestContext};

const REFUSAL: &str = "That action is not available yet";

fn preview_focused(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    let state = flea::jsondoc::parse(&read(shell, cx, "keyDeliveryState", &[])).unwrap();
    state
        .get("preview")
        .and_then(|preview| preview.get("focused"))
        == Some(&Json::Bool(true))
}

fn quiet(shell: &Entity<Shell>, cx: &mut VisualTestContext, keys: &[&str]) {
    for key in keys {
        cx.simulate_keystrokes(key);
        pump(cx);
        assert!(
            preview_focused(shell, cx),
            "{key} keeps the column preview focused"
        );
        assert_ne!(
            read(shell, cx, "lastMessage", &[]),
            REFUSAL,
            "{key} is silent"
        );
        assert_eq!(read(shell, cx, "cursor", &[]), "0", "{key} leaves the row");
    }
}

#[gpui::test]
fn focused_column_pdf_keeps_tab_and_takes_zoom_keys_silently(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_threads(|| {
        let (shell, cx, _) = super::pb_preview_fixture::pdf(cx);
        cx.simulate_keystrokes("ctrl-tab");
        pump(cx);
        assert!(preview_focused(&shell, cx), "Ctrl+Tab focuses the PDF");
        quiet(&shell, cx, &["tab", "shift-tab", "shift-=->+", "-", "="]);
        assert_eq!(read(&shell, cx, "columnPdfPage", &[]), "0");
        cx.simulate_keystrokes("ctrl-tab");
        pump(cx);
        assert!(!preview_focused(&shell, cx), "Ctrl+Tab still returns");
        assert_eq!(read(&shell, cx, "focusView", &[]), "list");
    });
}

#[gpui::test]
fn focused_column_text_takes_seek_keys_silently(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file(
            "code-rs.rs",
            "text-x-rust",
            "Rust source code",
            0o100644,
        )],
        1,
    );
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","lines":3"#);
    let asked = ask(&lines, "code");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":{},"total":3,"lines":[[0,"fn a() {{}}"],[0,"fn b() {{}}"],[0,"fn c() {{}}"]]}}"#,
            field(asked, "token"),
            field(asked, "first")
        ),
    );
    pump(cx);
    cx.simulate_keystrokes("ctrl-tab");
    pump(cx);
    assert!(preview_focused(&shell, cx), "Ctrl+Tab focuses the text");
    quiet(&shell, cx, &["h", "left", "right", "tab", "shift-tab"]);
}

// EY-331: htmlState answers for Quick Look's HTML, so the capture can wait on it and see R switch to Source.
#[gpui::test]
fn quick_look_html_state_follows_r_to_source(cx: &mut TestAppContext) {
    use super::document_keys_fixture as fixture;
    crate::g2::doc_preview::render::without_threads(|| {
        cx.executor().allow_parking();
        let root = fixture::Root::new();
        let dir = root.0.to_string_lossy().into_owned();
        let text = (0..61).map(|n| format!("<p>{n}</p>\n")).collect::<String>();
        std::fs::write(root.0.join("index.html"), &text).unwrap();
        let (shell, cx) = open(cx, &dir);
        super::test_shell::land(
            &shell,
            cx,
            &dir,
            vec![file("index.html", "text-html", "HTML document", 0o100644)],
            1,
        );
        let lines = local(&shell, cx, &dir);
        meta(&shell, cx, &lines, r#","lines":61"#);
        let column = fixture::view(&shell, cx, false);
        fixture::seed(&column, &text, cx);
        fixture::ready(&column, cx);
        let column_state = flea::jsondoc::parse(&read(&shell, cx, "htmlState", &["true"])).unwrap();
        assert_eq!(
            column_state.get("facts").and_then(Json::as_str),
            Some("HTML document · 600 B · 61 lines"),
            "the reader reports the compact facts the mounted HTML column draws"
        );
        cx.simulate_keystrokes("space");
        pump(cx);
        let look = fixture::view(&shell, cx, true);
        fixture::seed(&look, &text, cx);
        fixture::ready(&look, cx);
        let state = |cx: &mut VisualTestContext| {
            flea::jsondoc::parse(&read(&shell, cx, "htmlState", &["false"])).unwrap()
        };
        let rendered = state(cx);
        assert_eq!(
            rendered.get("view").and_then(Json::as_str),
            Some("rendered")
        );
        assert_eq!(rendered.get("visible"), Some(&Json::Bool(true)));
        assert_eq!(rendered.get("mark").and_then(Json::as_str), Some("html"));
        assert_eq!(rendered.get("facts").and_then(Json::as_str), Some(""));
        cx.simulate_keystrokes("r");
        pump(cx);
        assert_eq!(state(cx).get("view").and_then(Json::as_str), Some("source"));
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    });
}

// htmlState(true) answers for the focused pane's column HTML, the column half of IpcPreviewState.htmlState.
#[gpui::test]
fn column_html_state_answers_the_column(cx: &mut TestAppContext) {
    use super::document_keys_fixture as fixture;
    crate::g2::doc_preview::render::without_threads(|| {
        cx.executor().allow_parking();
        let root = fixture::Root::new();
        let dir = root.0.to_string_lossy().into_owned();
        let text = (0..61).map(|n| format!("<p>{n}</p>\n")).collect::<String>();
        std::fs::write(root.0.join("index.html"), &text).unwrap();
        let (shell, cx) = open(cx, &dir);
        super::test_shell::land(
            &shell,
            cx,
            &dir,
            vec![file("index.html", "text-html", "HTML document", 0o100644)],
            1,
        );
        let lines = local(&shell, cx, &dir);
        meta(&shell, cx, &lines, r#","lines":61"#);
        let column = fixture::view(&shell, cx, false);
        fixture::seed(&column, &text, cx);
        fixture::ready(&column, cx);
        let state = flea::jsondoc::parse(&read(&shell, cx, "htmlState", &["true"])).unwrap();
        assert_eq!(state.get("column"), Some(&Json::Bool(true)), "{state:?}");
        assert_eq!(state.get("visible"), Some(&Json::Bool(true)));
        assert_eq!(state.get("view").and_then(Json::as_str), Some("rendered"));
        assert_eq!(state.get("mark").and_then(Json::as_str), Some("html"));
        let path = state.get("path").and_then(Json::as_str).unwrap_or_default();
        assert!(path.ends_with("index.html"), "{path}");
        assert_eq!(
            read(&shell, cx, "htmlState", &["false"]),
            "{}",
            "no card is open"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    });
}
