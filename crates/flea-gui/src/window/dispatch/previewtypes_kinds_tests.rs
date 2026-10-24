//! Each row kind mounts its own view in the preview column of the production Shell, walked with the Down key.
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::window::{
    selection_preview::{docs::Doc, Mounted},
    u07_tests::read,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::path::{Path, PathBuf};

/// A scratch folder holding its marker; every removal checks the marker and an absolute, non-root path first.
pub(super) struct Root(pub(super) PathBuf);
impl Root {
    pub(super) fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("flea-previewtypes-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(".flea-test-sandbox"), b"").unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let path: &Path = &self.0;
        if path.is_absolute() && path.parent().is_some() && path.join(".flea-test-sandbox").is_file() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn doc_name(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> &'static str {
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let preview = preview.read(cx);
        let frame = preview.pending.as_ref().or(preview.drawn.as_ref());
        match frame.map(|frame| &frame.mounted) {
            Some(Mounted::Doc(Doc::Code(_))) => "code",
            Some(Mounted::Doc(Doc::Markdown(_))) => "markdown",
            Some(Mounted::Doc(Doc::Table(_))) => "table",
            Some(Mounted::Doc(Doc::Html(_))) => "html",
            Some(Mounted::Doc(Doc::Archive(_))) => "archive",
            Some(Mounted::Doc(Doc::Office(_))) => "office",
            Some(_) => "frame",
            None => "none",
        }
    })
}

#[gpui::test]
fn markdown_csv_html_archive_symlink_and_data_rows_draw_their_own_preview(cx: &mut TestAppContext) {
    // The markdown parse and picture threads are real, so their wakes come from outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("kinds");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("notes.md"), "# Notes\n\nbody text\n").unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![
        file("data.csv", "text-csv", "CSV document", 0o100644),
        file("page.html", "text-html", "HTML document", 0o100644),
        file("pack.zip", "application-zip", "Zip archive", 0o100644),
        file("link.png", "image-png", "PNG image", 0o120777),
        file("blob.bin", "application-octet-stream", "Data", 0o100644),
        file("notes.md", "text-markdown", "Markdown document", 0o100644),
    ];
    super::test_shell::land(&shell, cx, &dir, rows, 1);

    // CSV asks the engine for raw records and draws them as a table.
    let lines = local(&shell, cx, &dir);
    assert_eq!(doc_name(&shell, cx), "table");
    let table = ask(&lines, "code").to_owned();
    assert!(table.contains(r#""csv":","#) && table.contains("data.csv"), "{table}");
    meta(&shell, cx, &lines, r#","lines":2"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":2,"lines":[[0,"a,b"],[0,"1,2"]],"records":2,"firstRecord":1}}"#,
            field(&table, "token")
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "text");

    // HTML asks the page renderer for the column's slot, never raw source lines.
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    assert_eq!(doc_name(&shell, cx), "html");
    let html = ask(&lines, "html");
    assert!(html.contains(r#""slot":"column""#) && html.contains("page.html"), "{html}");

    // An archive lists the names its meta reply carries, with the entry count in the facts.
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    assert_eq!(doc_name(&shell, cx), "archive");
    assert!(ask(&lines, "meta").contains(r#""archive":true"#));
    meta(
        &shell,
        cx,
        &lines,
        r#","entries":3,"names":[{"n":"docs","d":true},{"n":"a.txt","d":false},{"n":"b.txt","d":false}]"#,
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "archive");
    assert_eq!(read(&shell, cx, "columnArchiveNames", &[]), "docs|a.txt|b.txt");
    let facts = read(&shell, cx, "previewFacts", &[]);
    assert!(facts.starts_with("Kind=Zip archive|") && facts.contains("|Entries=3"), "{facts}");

    // A symlink states its Kind, Target and what it points at.
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    meta(&shell, cx, &lines, r#","target":"/elsewhere/photo.png""#);
    answer(&shell, cx, r#"{"t":"thumbed","row":3,"file":""}"#);
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "symlink");
    let facts = read(&shell, cx, "previewFacts", &[]);
    for fact in ["Kind=Symbolic link|", "Target=/elsewhere/photo.png|", "Points at=File|", "Mode="] {
        assert!(facts.contains(fact), "{fact} in {facts}");
    }

    // A Data row draws no preview and no read failure, with Kind Data and Mode.
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    meta(&shell, cx, &lines, "");
    answer(&shell, cx, r#"{"t":"thumbed","row":4,"file":""}"#);
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "unsupported");
    assert_eq!(read(&shell, cx, "columnFailure", &[]), "");
    let facts = read(&shell, cx, "previewFacts", &[]);
    assert!(facts.starts_with("Kind=Data|") && facts.contains("|Mode="), "{facts}");
    assert!(!facts.contains("Lines"), "a Data row counts no lines: {facts}");

    // Markdown reads and parses the file itself and draws the rendered document; last, so its decoder thread outlives the walk.
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    assert_eq!(doc_name(&shell, cx), "markdown");
    meta(&shell, cx, &lines, r#","lines":3"#);
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "text");
    assert_eq!(read(&shell, cx, "columnFailure", &[]), "");
    let facts = read(&shell, cx, "previewFacts", &[]);
    assert!(facts.starts_with("Kind=Markdown document|"), "{facts}");
}
