//! Preview keys travel through the mounted root and preserve the listing behind them.
use super::previewtypes_tests::{answer, file, local, open, pump};
use crate::window::u07_tests::read;
use gpui::TestAppContext;

#[gpui::test]
fn pb_preview_ctrl_tab_returns_from_every_column_kind(cx: &mut TestAppContext) {
    crate::g2::pv2::render::without_threads(|| {
        let (shell, cx) = open(cx, "/files");
        let rows = vec![
            file("plain.bin", "application-octet-stream", "Data", 0o100644),
            file("clip.mp4", "video-x-generic", "Video", 0o100644),
            file("pages.pdf", "application-pdf", "PDF document", 0o100644),
        ];
        super::test_shell::land(&shell, cx, "/files", rows, 1);
        local(&shell, cx, "/files");
        for row in 0..3 {
            if row > 0 {
                cx.simulate_keystrokes("down");
            }
            pump(cx);
            cx.simulate_keystrokes("ctrl-tab");
            pump(cx);
            assert_ne!(read(&shell, cx, "focusView", &[]), "list");
            cx.simulate_keystrokes("ctrl-tab");
            pump(cx);
            assert_eq!(
                read(&shell, cx, "focusView", &[]),
                "list",
                "Ctrl+Tab returns from row {row}"
            );
            assert_eq!(read(&shell, cx, "cursor", &[]), row.to_string());
        }
    });
}

#[gpui::test]
fn pb_preview_focused_pdf_scrolls_and_return_expands(cx: &mut TestAppContext) {
    crate::g2::pv2::render::without_threads(|| {
        let (shell, cx, _) = super::pb_preview_fixture::pdf(cx);
        cx.simulate_keystrokes("ctrl-tab");
        let scroll = |cx: &mut gpui::VisualTestContext| {
            flea::jsondoc::parse(&read(&shell, cx, "pdfState", &["false"]))
                .unwrap()
                .get("scrollY")
                .and_then(flea::jsondoc::Json::as_f64)
                .unwrap()
        };
        assert_eq!(scroll(cx), 0.);
        cx.simulate_keystrokes("down");
        pump(cx);
        assert!(scroll(cx) > 0., "Down scrolls the PDF column");
        assert_eq!(read(&shell, cx, "cursor", &[]), "0");
        cx.simulate_keystrokes("up");
        pump(cx);
        assert_eq!(scroll(cx), 0., "Up scrolls back");
        for key in ["enter", "return"] {
            cx.simulate_keystrokes(key);
            pump(cx);
            assert_eq!(
                read(&shell, cx, "previewOpen", &[]),
                "true",
                "{key} expands the PDF"
            );
            cx.simulate_keystrokes("escape");
            pump(cx);
            cx.simulate_keystrokes("ctrl-tab");
        }
    });
}

#[gpui::test]
fn pb_preview_menu_compare_reaches_copy_patch(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![
            file("a.txt", "text-plain", "Text", 0o100644),
            file("b.txt", "text-plain", "Text", 0o100644),
        ],
        1,
    );
    local(&shell, cx, "/files");
    cx.simulate_keystrokes("shift-down");
    cx.simulate_keystrokes("shift-f10");
    pump(cx);
    super::menus_tests::answer(&shell, cx, &[0o100644, 0o100644]);
    super::menus_tests::choose(&shell, cx, "compare");
    super::menus_tests::validated(&shell, cx, "compare", &["/files/a.txt", "/files/b.txt"]);
    let lines = pump(cx);
    let ask = super::previewtypes_tests::ask(&lines, "compare");
    let token = super::previewtypes_tests::field(ask, "token");
    let patch = "--- a.txt\n+++ b.txt\n@@ -1 +1 @@\n-old\n+new\n";
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"compare","token":{token},"state":"diff","patch":"{}"}}"#,
            flea::json::escape(patch)
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    cx.simulate_keystrokes("y");
    pump(cx);
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(patch)
    );
}

#[gpui::test]
fn pb_preview_panel_up_down_and_j_k_leave_the_listing_and_focus_alone(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("a.bin", "application-octet-stream", "Data", 0o100644); 3],
        1,
    );
    pump(cx);
    for _ in 0..3 {
        if read(&shell, cx, "focusView", &[]) == "chrome" {
            break;
        }
        cx.simulate_keystrokes("tab");
        pump(cx);
    }
    assert_eq!(read(&shell, cx, "focusView", &[]), "chrome");
    let focus = read(&shell, cx, "focusStop", &[]);
    for key in ["down", "up", "j", "k"] {
        cx.simulate_keystrokes(key);
        pump(cx);
        assert_eq!(
            read(&shell, cx, "cursor", &[]),
            "0",
            "{key} stays quiet like ChromeKeys.js"
        );
        assert_eq!(read(&shell, cx, "focusStop", &[]), focus);
    }
}
