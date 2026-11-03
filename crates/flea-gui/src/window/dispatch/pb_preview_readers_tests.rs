//! The real window's adapter set answers the preview reader contract in its absent state.
use crate::state::Shell;
use crate::window::u07_tests::read;
use gpui::{Entity, TestAppContext, VisualTestContext};

// The reader answers Target not found until the first listing settles, as v0.3.12 did, so each case lands one first.
fn landed(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = super::previewtypes_tests::open(cx, "/files");
    let rows = vec![super::previewtypes_tests::file(
        "a.bin",
        "application-octet-stream",
        "Data",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    (shell, cx)
}

macro_rules! absent {
    ($test:ident, $name:literal, [$($arg:literal),*], $value:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            let (shell, cx) = landed(cx);
            assert_eq!(read(&shell, cx, $name, &[$($arg),*]), $value);
        }
    };
}
absent!(pb_preview_duration_absent, "previewDuration", [], "0");
absent!(
    pb_preview_markdown_view_absent,
    "previewMarkdownView",
    [],
    ""
);
absent!(pb_preview_end_gap_absent, "previewEndGap", [], "-1");
absent!(pb_preview_figures_absent, "previewFigures", [], "");
absent!(
    pb_preview_block_kinds_absent,
    "previewBlockKinds",
    ["false"],
    ""
);

#[gpui::test]
fn pb_preview_pdf_state_overlay_absent(cx: &mut TestAppContext) {
    let (shell, cx) = landed(cx);
    assert_eq!(read(&shell, cx, "pdfState", &["true"]), "null");
}

#[gpui::test]
fn pb_preview_markdown_readers_follow_the_mounted_document_and_view_key(cx: &mut TestAppContext) {
    use super::previewtypes_tests::{file, local, meta, open, pump};
    use crate::g2::pv1::doc::View;
    use std::time::{Duration, Instant};
    cx.executor().allow_parking();
    let root = super::previewtypes_kinds_tests::Root::new("pb-readers");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(
        root.0.join("notes.md"),
        "# Title\n\nA paragraph.\n\n```rust\nfn main() {}\n```\n",
    )
    .unwrap();
    let (shell, cx) = open(cx, &dir);
    super::test_shell::land(
        &shell,
        cx,
        &dir,
        vec![file(
            "notes.md",
            "text-markdown",
            "Markdown document",
            0o100644,
        )],
        1,
    );
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","lines":7"#);
    cx.simulate_keystrokes("space");
    pump(cx);
    let markdown = shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        match &look.subject().unwrap().body.doc().unwrap().view {
            View::Markdown(markdown) => markdown.clone(),
            _ => panic!("Markdown mounted"),
        }
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cx.read(|cx| markdown.read(cx).doc.content_ready()) {
        assert!(Instant::now() < deadline, "the document parsed");
        pump(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        read(&shell, cx, "previewBlockKinds", &["false"]),
        r#"["heading","run","fence:rust"]"#
    );
    assert_eq!(read(&shell, cx, "previewFigures", &[]), "");
    assert_eq!(read(&shell, cx, "previewMarkdownView", &[]), "rendered");
    cx.simulate_keystrokes("r");
    pump(cx);
    assert_eq!(read(&shell, cx, "previewMarkdownView", &[]), "source");
    cx.simulate_keystrokes("escape");
    pump(cx);
    assert_eq!(read(&shell, cx, "previewMarkdownView", &[]), "");
}
