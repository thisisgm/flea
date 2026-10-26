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

#[gpui::test]
fn f_and_b_page_a_table_by_its_viewport_height(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file("data.csv", "text-csv", "CSV document", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let table = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":200"#);
    let body = (0..200).map(|n| format!(r#"[0,"{n},x"]"#)).collect::<Vec<_>>().join(",");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":200,"lines":[{body}],"records":200,"firstRecord":1}}"#,
            field(&table, "token")
        ),
    );
    pump(cx);
    let scroll = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            let preview = shell.selection_preview(0).expect("the column is mounted");
            match preview.read(cx).drawn.as_ref().map(|frame| &frame.mounted) {
                Some(Mounted::Doc(Doc::Table(view))) => {
                    let view = view.read(cx);
                    (view.scroll.1, view.frame.1 - view.feed.geometry.head_height)
                }
                _ => panic!("the table is drawn"),
            }
        })
    };
    let (top, height) = scroll(cx);
    assert!(top == 0. && height > 0., "the table starts at its top in a laid-out frame: {top} {height}");
    cx.simulate_keystrokes("shift-f->F");
    pump(cx);
    // PreviewScroll.qml pageBy moves contentY by the viewport's own height, so no row is skipped unseen.
    assert_eq!(scroll(cx).0, height, "F moves the table by one viewport height");
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(scroll(cx).0, 0., "B pages back to the top");
    cx.simulate_keystrokes("shift-b->B");
    pump(cx);
    assert_eq!(scroll(cx).0, 0., "B at the top stays at the top");
    let end = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        match preview.read(cx).drawn.as_ref().map(|frame| &frame.mounted) {
            Some(Mounted::Doc(Doc::Table(view))) => {
                let view = view.read(cx);
                view.feed.body_rows() as f64 * view.feed.geometry.row_height - height
            }
            _ => panic!("the table is drawn"),
        }
    });
    assert!(end > 2. * height, "the table is taller than two viewports: {end} {height}");
    for _ in 0..40 {
        cx.simulate_keystrokes("shift-f->F");
        pump(cx);
    }
    assert_eq!(scroll(cx).0, end, "F past the end stops with the last row on the bottom edge");
}

/// A two-frame GIF, red then blue, half a second a frame.
const ANIMATED_GIF: [u8; 101] = [
    71, 73, 70, 56, 57, 97, 8, 0, 8, 0, 240, 0, 0, 255, 0, 0, 255, 255, 255, 33, 255, 11, 78, 69,
    84, 83, 67, 65, 80, 69, 50, 46, 48, 3, 1, 0, 0, 0, 33, 249, 4, 0, 50, 0, 0, 0, 44, 0, 0, 0,
    0, 8, 0, 8, 0, 0, 2, 7, 132, 143, 169, 203, 237, 93, 0, 0, 33, 249, 4, 0, 50, 0, 0, 0, 44, 0,
    0, 0, 0, 8, 0, 8, 0, 128, 0, 0, 255, 255, 255, 255, 2, 7, 132, 143, 169, 203, 237, 93, 0, 0,
    59,
];

/// A 4 by 4 PNG, the still frame a thumbnailer caches for the GIF.
const STILL_PNG: [u8; 73] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 2, 0,
    0, 0, 38, 147, 9, 41, 0, 0, 0, 16, 73, 68, 65, 84, 120, 156, 99, 56, 81, 161, 1, 71, 12, 196,
    113, 0, 83, 82, 22, 129, 25, 170, 14, 207, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Frames and real time for a picture decode, which runs on the background executor.
fn settle_pictures(cx: &mut VisualTestContext) {
    for _ in 0..40 {
        cx.executor().advance_clock(std::time::Duration::from_millis(300));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[gpui::test]
fn an_animated_gif_animates_in_the_column_and_stops_when_the_preview_leaves_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("gifanim");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("anim.gif"), ANIMATED_GIF).unwrap();
    std::fs::write(root.0.join("notes.txt"), "hello\n").unwrap();
    let still = root.0.join("still.png");
    std::fs::write(&still, STILL_PNG).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![
        file("anim.gif", "image-gif", "GIF image", 0o100644),
        file("notes.txt", "text-plain", "Plain text document", 0o100644),
    ];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","frames":4,"w":8,"h":8"#);
    answer(&shell, cx, &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, still.display()));
    settle_pictures(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image");
    let frames = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview
            .read(cx)
            .drawn
            .as_ref()
            .and_then(|frame| frame.animation.as_ref().map(|image| image.frame_count()))
    });
    assert_eq!(frames, Some(2), "the GIF's own frames load over its still frame");
    assert!(
        cx.debug_bounds("column-preview-animation").is_some(),
        "the moving layer draws in the frame"
    );
    let moving = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        let frame = preview.read(cx).drawn.as_ref().expect("the GIF is drawn");
        std::sync::Arc::downgrade(frame.animation.as_ref().expect("its frames are in"))
    });
    cx.simulate_keystrokes("down");
    let lines = pump(cx);
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":1"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":1,"lines":[[0,"hello"]]}}"#,
            field(&code, "token")
        ),
    );
    settle_pictures(cx);
    let after = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview.read(cx).drawn.as_ref().map(|frame| {
            (
                frame.subject.row.name.clone(),
                frame.animation.as_ref().map(|image| image.frame_count()),
            )
        })
    });
    assert_eq!(
        after,
        Some(("notes.txt".into(), None)),
        "leaving the GIF stops its animation with the frame"
    );
    assert!(
        cx.debug_bounds("column-preview-animation").is_none(),
        "no moving layer draws for the next file"
    );
    assert!(
        moving.upgrade().is_none(),
        "the retired GIF frame releases its decoded frames from the asset cache"
    );
}

#[gpui::test]
fn a_gif_past_the_raster_budget_stays_a_still_in_the_column(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("gifhuge");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("anim.gif"), ANIMATED_GIF).unwrap();
    let still = root.0.join("still.png");
    std::fs::write(&still, STILL_PNG).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("anim.gif", "image-gif", "GIF image", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","frames":4,"w":4000,"h":4000"#);
    answer(&shell, cx, &format!(r#"{{"t":"thumbed","row":0,"file":"{}"}}"#, still.display()));
    settle_pictures(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image");
    let moving = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview.read(cx).drawn.as_ref().map(|frame| frame.animation_source.is_some())
    });
    assert_eq!(moving, Some(false), "four 4000 px frames are never decoded for the column");
    assert!(cx.debug_bounds("column-preview-animation").is_none(), "the still stands alone");
}

#[gpui::test]
fn an_empty_text_file_draws_the_document_glyph_in_the_column(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    let rows = vec![file("empty.txt", "text-plain", "Plain text document", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    let code = ask(&lines, "code").to_owned();
    meta(&shell, cx, &lines, r#","lines":0"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":0,"lines":[]}}"#,
            field(&code, "token")
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "text");
    assert!(
        cx.debug_bounds("column-preview-empty-glyph").is_some(),
        "an empty text file draws the document glyph centred in the frame"
    );
}

#[gpui::test]
fn an_image_that_fails_to_decode_keeps_its_image_facts(cx: &mut TestAppContext) {
    // The picture decodes on the asset loader's thread, outside the deterministic scheduler.
    cx.executor().allow_parking();
    let root = Root::new("badimage");
    let dir = root.0.to_string_lossy().into_owned();
    std::fs::write(root.0.join("broken.png"), b"not a picture").unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("broken.png", "image-png", "PNG image", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","w":2,"h":2"#);
    answer(&shell, cx, r#"{"t":"thumbed","row":0,"file":""}"#);
    pump(cx);
    let failed = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).expect("the column is mounted");
        preview.read(cx).drawn.as_ref().is_some_and(|frame| frame.failed)
    });
    assert!(failed, "the broken picture is a failed frame");
    // PreviewColumn.qml failure covers only text, PDF, HTML and archive reads, so the facts stay the image's.
    let facts = read(&shell, cx, "previewFacts", &[]);
    assert!(facts.contains("|Modified=") && facts.contains("|Pixels="), "{facts}");
    // The column's own state and Kind readers; previewState and previewKind are the Quick Look's.
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "image", "the state stays the row's kind");
    assert!(facts.starts_with("Kind=PNG image|"), "Kind is the row's description: {facts}");
    assert_eq!(read(&shell, cx, "columnFailure", &[]), "", "a failed picture draws no read failure");
}
