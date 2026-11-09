//! v0.3.12 Preview.qml retains markdownSource until Quick Look closes.
use super::{
    preview_tests::{close, file, opened, press, Lines},
    test_shell::{land, set_cursor},
};
use crate::{state::Shell, window::u07_tests::read};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn fixture(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines) {
    let (shell, cx, requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![
            file("a.md", "text-markdown"),
            file("b.md", "text-markdown"),
            file("c.txt", "text-plain"),
            file("d.img", "application-octet-stream"),
            file("e.md", "text-markdown"),
        ],
        1,
    );
    set_cursor(&shell, cx, 0);
    press(cx, "space");
    assert_markdown(&shell, cx, "a.md", "rendered");
    (shell, cx, requests)
}

fn assert_markdown(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str, shown: &str) {
    assert_eq!(read(shell, cx, "previewOpen", &[]), "true");
    shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        assert_eq!(look.subject().unwrap().name, name);
    });
    assert_eq!(read(shell, cx, "previewMarkdownView", &[]), shown);
}

#[gpui::test]
fn ds_ql_source_carry_follows_the_next_markdown(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_markdown_threads(|| {
        let (shell, cx, _requests) = fixture(cx);
        press(cx, "r");
        assert_markdown(&shell, cx, "a.md", "source");
        press(cx, "down");
        assert_markdown(&shell, cx, "b.md", "source");
        press(cx, "r");
        assert_markdown(&shell, cx, "b.md", "rendered");
        press(cx, "up");
        assert_markdown(&shell, cx, "a.md", "rendered");
        close(&shell, cx);
    });
}

#[gpui::test]
fn ds_ql_source_carry_survives_other_file_kinds(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_markdown_threads(|| {
        let (shell, cx, _requests) = fixture(cx);
        press(cx, "r");
        for (cursor, kind) in [("1", "text"), ("2", "text"), ("3", "unsupported")] {
            press(cx, "down");
            assert_eq!(read(&shell, cx, "cursor", &[]), cursor);
            assert_eq!(read(&shell, cx, "previewKind", &[]), kind);
        }
        press(cx, "down");
        assert_markdown(&shell, cx, "e.md", "source");
        close(&shell, cx);
    });
}

#[gpui::test]
fn ds_ql_source_carry_close_then_reopen_starts_rendered(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_markdown_threads(|| {
        let (shell, cx, _requests) = fixture(cx);
        for key in ["escape", "space"] {
            press(cx, "r");
            assert_markdown(&shell, cx, "a.md", "source");
            press(cx, key);
            assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
            press(cx, "space");
            assert_markdown(&shell, cx, "a.md", "rendered");
        }
        close(&shell, cx);
    });
}
