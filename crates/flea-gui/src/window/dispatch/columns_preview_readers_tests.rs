//! Named preview readers answer the production document, including source mode and the archive inset.
use super::previewtypes_tests as host;
use crate::window::{
    selection_preview::{docs::Doc, Mounted},
    u07_tests::read,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn markdown(
    shell: &Entity<crate::state::Shell>,
    cx: &VisualTestContext,
) -> Option<Entity<crate::g2::pv2::markdown::MarkdownView>> {
    shell.read_with(cx, |shell, cx| {
        match &shell.selection_preview(0)?.read(cx).drawn.as_ref()?.mounted {
            Mounted::Doc(Doc::Markdown(markdown)) => Some(markdown.clone()),
            _ => None,
        }
    })
}

#[gpui::test]
fn pb_columns2_markdown_reader_reads_the_mounted_document_and_source(cx: &mut TestAppContext) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join(".superpowers/PB-COLUMNS-2");
    std::fs::create_dir_all(&base).unwrap();
    let made = std::process::Command::new("mktemp")
        .args(["-d", base.join("reader.XXXXXXXX").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(&base) && root != base);
    std::fs::write(root.join(".flea-test-root"), "").unwrap();
    let raw = "# Notes\n###\ntext\n";
    std::fs::write(root.join("notes.md"), raw).unwrap();
    let path = root.to_str().unwrap();
    let (shell, cx) = host::open(cx, path);
    // The real Markdown worker and decoder reply from operating system threads.
    cx.executor().allow_parking();
    super::test_shell::land(
        &shell,
        cx,
        path,
        vec![host::file("notes.md", "text-markdown", "Markdown", 0)],
        1,
    );
    let lines = host::local(&shell, cx, path);
    host::meta(&shell, cx, &lines, r#","lines":4"#);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        host::pump(cx);
        if markdown(&shell, cx)
            .is_some_and(|view| view.read_with(cx, |view, _| view.doc.text.as_deref() == Some(raw)))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the real Markdown worker did not publish the fixture"
        );
        std::thread::yield_now();
    }
    assert_eq!(read(&shell, cx, "columnMarkdownText", &[]), "Notes\n\ntext");
    let view = markdown(&shell, cx).unwrap();
    view.update(cx, |view, cx| {
        view.set_view(crate::g2::pv2::markdown::View::Source, cx)
    });
    assert_eq!(
        read(&shell, cx, "columnMarkdownText", &[]),
        raw.trim_end_matches('\n')
    );
    assert!(read(&shell, cx, "columnArchiveRect", &[]).is_empty());
}

#[gpui::test]
fn pb_columns2_archive_reader_returns_the_mounted_entries_rectangle(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file("pack.zip", "package-x-generic", "Archive", 0)],
        1,
    );
    let lines = host::local(&shell, cx, "/files");
    host::meta(
        &shell,
        cx,
        &lines,
        r#","entries":1,"names":[{"n":"member.txt","d":false}]"#,
    );
    host::pump(cx);
    assert_eq!(read(&shell, cx, "columnArchiveNames", &[]), "member.txt");
    let expected = shell.read_with(cx, |shell, cx| {
        let view = shell.selection_preview(0).unwrap();
        let view = view.read(cx);
        let bounds = view.frame_rect.borrow().unwrap();
        let Mounted::Doc(Doc::Archive(archive)) = &view.drawn.as_ref().unwrap().mounted else {
            panic!("archive mounted");
        };
        let inset = gpui::px(archive.read(cx).metrics.inset);
        format!(
            "{} {} {} {}",
            f32::from(bounds.left() + inset).round(),
            f32::from(bounds.top() + inset).round(),
            f32::from(bounds.size.width - inset * 2.).round(),
            f32::from(bounds.size.height - inset * 2.).round()
        )
    });
    assert_eq!(read(&shell, cx, "columnArchiveRect", &[]), expected);
    assert!(read(&shell, cx, "columnMarkdownText", &[]).is_empty());
}
