//! The legacy reader answers the live column's mounted, ready Markdown view.
use super::{previewtypes_tests as host, test_shell};
use crate::window::{
    selection_preview::{docs::Doc, Mounted},
    u07_tests::read,
};
use gpui::TestAppContext;

#[gpui::test]
fn pb_columns1_markdown_view_reader_is_mounted_in_the_normal_root(cx: &mut TestAppContext) {
    let (shell, cx) = test_shell::open_root_with(cx, r#""keys":"default","view":"columns""#, None);
    // Before the first listing the window answers "Target not found." (v0.3.12), so the root settles first.
    test_shell::land(
        &shell,
        cx,
        "/files",
        vec![test_shell::row("notes.md", false)],
        1,
    );
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "");
}

/// A sandboxed notes.md landed in the column, once its Markdown view is mounted and ready.
fn ready_markdown(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<crate::state::Shell>,
    &mut gpui::VisualTestContext,
    gpui::Entity<crate::g2::doc_preview::markdown::MarkdownView>,
) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join(".superpowers/PB-COLUMNS-1");
    std::fs::create_dir_all(&base).unwrap();
    let made = std::process::Command::new("mktemp")
        .args(["-d", base.join("markdown.XXXXXXXX").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(&base) && root != base);
    std::fs::write(root.join(".flea-test-root"), "").unwrap();
    std::fs::write(root.join("notes.md"), "# Notes\n\nbody\n").unwrap();
    let path = root.to_str().unwrap();
    let (shell, cx) = host::open(cx, path);
    cx.executor().allow_parking();
    test_shell::land(
        &shell,
        cx,
        path,
        vec![host::file("notes.md", "text-markdown", "Markdown", 0)],
        1,
    );
    let lines = host::local(&shell, cx, path);
    host::meta(&shell, cx, &lines, r#", "lines":4"#);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let markdown = loop {
        host::pump(cx);
        let markdown = shell.read_with(cx, |shell, cx| {
            let preview = shell.selection_preview(0)?.read(cx);
            match &preview.drawn.as_ref()?.mounted {
                Mounted::Doc(Doc::Markdown(markdown)) if markdown.read(cx).doc.content_ready() => {
                    Some(markdown.clone())
                }
                _ => None,
            }
        });
        if let Some(markdown) = markdown {
            break markdown;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Markdown worker did not finish"
        );
        std::thread::yield_now();
    };
    (shell, cx, markdown)
}

#[gpui::test]
fn pb_columns1_markdown_view_reader_follows_ready_document_source_and_visibility(
    cx: &mut TestAppContext,
) {
    let (shell, cx, markdown) = ready_markdown(cx);
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "rendered");
    markdown.update(cx, |markdown, cx| {
        markdown.set_view(crate::g2::doc_preview::markdown::View::Source, cx)
    });
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "source");
    markdown.update(cx, |markdown, cx| markdown.set_active(false, cx));
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "");
    markdown.update(cx, |markdown, cx| markdown.set_active(true, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::List, window, cx)
        })
    });
    host::pump(cx);
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "");
}

// Judge R: previewBlockKinds(column) reads the column's Markdown for true (or 1) and Quick Look's for false, as IpcPreviewState does.
#[gpui::test]
fn fixreaders_preview_block_kinds_true_reads_the_column_markdown(cx: &mut TestAppContext) {
    let (shell, cx, _markdown) = ready_markdown(cx);
    assert_eq!(
        read(&shell, cx, "previewBlockKinds", &["true"]),
        r#"["heading","run"]"#
    );
    assert_eq!(
        read(&shell, cx, "previewBlockKinds", &["1"]),
        r#"["heading","run"]"#
    );
    // Quick Look is closed, so the overlay arm has no document.
    assert_eq!(read(&shell, cx, "previewBlockKinds", &["false"]), "");
    assert_eq!(read(&shell, cx, "previewBlockKinds", &["0"]), "");
    assert!(read(&shell, cx, "previewBlockKinds", &[])
        .starts_with("Too few arguments provided (1 required but 0 were provided.)"));
}
