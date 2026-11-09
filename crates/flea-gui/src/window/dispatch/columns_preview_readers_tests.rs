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
) -> Option<Entity<crate::g2::doc_preview::markdown::MarkdownView>> {
    shell.read_with(cx, |shell, cx| {
        match &shell.selection_preview(0)?.read(cx).drawn.as_ref()?.mounted {
            Mounted::Doc(Doc::Markdown(markdown)) => Some(markdown.clone()),
            _ => None,
        }
    })
}

#[gpui::test]
fn columns_text_reader_preserves_the_native_too_large_sentinel(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    let mut row = host::file("big.txt", "text-plain", "Text", 0o100644);
    row.size = 2 * 1024 * 1024;
    super::test_shell::land(&shell, cx, "/files", vec![row], 1);
    let lines = host::local(&shell, cx, "/files");
    host::meta(&shell, cx, &lines, r#","lines":0"#);
    host::pump(cx);
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap().read(cx);
        let Mounted::Doc(Doc::Code(code)) = &preview.drawn.as_ref().unwrap().mounted else {
            panic!("the too-large code document remains mounted")
        };
        assert_eq!(
            code.read(cx).refusal(),
            Some(flea::ui_logic_doc_preview::refusal::Refusal::TooLarge)
        );
    });
    assert_eq!(read(&shell, cx, "columnTextLines", &[]), "too large");
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
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "rendered");
    let view = markdown(&shell, cx).unwrap();
    view.update(cx, |view, cx| {
        view.set_view(crate::g2::doc_preview::markdown::View::Source, cx)
    });
    assert_eq!(read(&shell, cx, "columnMarkdownView", &[]), "source");
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

// EY-406: the column steps entries as PreviewArchive does, a 1.8 caption line box (22 px at 12), not the 1.4 line pitch.
#[gpui::test]
fn columns_archive_entries_keep_the_v0312_pitch(cx: &mut TestAppContext) {
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
        r#","entries":2,"names":[{"n":"a.txt","d":false},{"n":"b.txt","d":false}]"#,
    );
    host::pump(cx);
    shell.read_with(cx, |shell, cx| {
        let view = shell.selection_preview(0).unwrap().read(cx);
        let Mounted::Doc(Doc::Archive(archive)) = &view.drawn.as_ref().unwrap().mounted else {
            panic!("archive mounted");
        };
        let archive = archive.read(cx);
        let caption = f32::from(archive.metrics.caption);
        assert_eq!(archive.metrics.row, (caption * 1.8).round(), "row pitch");
    });
}

#[gpui::test]
fn columns_picture_rect_reads_the_small_image_as_painted(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file("small.png", "image-png", "PNG image", 0o100644)],
        1,
    );
    let lines = host::local(&shell, cx, "/files");
    host::meta(&shell, cx, &lines, r#","w":64,"h":64"#);
    let preview = shell.read_with(cx, |shell, _| shell.selection_preview(0).unwrap());
    preview.update(cx, |view, cx| {
        let frame = view.pending.as_mut().or(view.drawn.as_mut()).unwrap();
        frame.thumb_answered = true;
        frame.source = None;
        frame.image = Some(std::sync::Arc::new(
            gpui::RenderImage::from_bgra(64, 64, vec![128; 64 * 64 * 4]).unwrap(),
        ));
        cx.notify();
    });
    host::pump(cx);
    let bounds = cx
        .debug_bounds("column-preview-image")
        .expect("the actual image lays out");
    // A small raster keeps its own pixels, one to each device pixel.
    let scale = cx.update(|window, _| f64::from(window.scale_factor()));
    let (w, h) = crate::preview_quality::own_size((64, 64), scale);
    assert_eq!(
        (f32::from(bounds.size.width), f32::from(bounds.size.height)),
        (w.round(), h.round())
    );
    assert_eq!(
        read(&shell, cx, "columnPictureRect", &[]),
        format!(
            "{} {} {} {}",
            f32::from(bounds.left()).round(),
            f32::from(bounds.top()).round(),
            w.round(),
            h.round()
        )
    );
}

#[gpui::test]
fn columns_play_button_keeps_listing_keys_and_selection_retires_the_player(
    cx: &mut TestAppContext,
) {
    use super::quick_look_body_tests::{Fake, Script};
    use flea::ui_logic_media_preview::frame::Event;
    use std::rc::Rc;

    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![
            host::file("clip.mp4", "video-mp4", "MPEG-4 video", 0o100644),
            host::file("note.txt", "text-plain", "Plain text", 0o100644),
        ],
        1,
    );
    let lines = host::local(&shell, cx, "/files");
    host::meta(&shell, cx, &lines, r#","ms":20000"#);
    let preview = shell.read_with(cx, |shell, _| shell.selection_preview(0).unwrap());
    let script = Rc::new(Script::default());
    preview.update(cx, |view, _| {
        view.set_media_backend(Rc::new(Fake(script.clone())));
        let frame = view.pending.as_mut().or(view.drawn.as_mut()).unwrap();
        frame.thumb_answered = true;
        frame.source = None;
    });
    host::pump(cx);
    let centre = read(&shell, cx, "columnPlayCentre", &[])
        .split_whitespace()
        .map(|v| v.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(centre.len(), 2);
    cx.simulate_click(
        gpui::point(gpui::px(centre[0]), gpui::px(centre[1])),
        gpui::Modifiers::none(),
    );
    for event in [
        Event::Loaded {
            video: true,
            duration_ms: 20_000,
        },
        Event::Playing,
    ] {
        assert!(script.feeds.borrow()[0].event(event));
    }
    host::pump(cx);
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "true");
    assert_eq!(
        super::preview_tests::context(&shell, cx),
        flea::keys::Context::Listing
    );
    assert!(
        cx.update(|window, cx| !preview.read(cx).focus.is_focused(window)),
        "the strip leaves listing focus in place"
    );
    cx.simulate_keystrokes("ctrl-a");
    host::pump(cx);
    let diagnostic = shell.read_with(cx, |shell, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let view = shell.selection_preview(0).unwrap().read(cx);
        (
            host.selected.clone(),
            host.input.band.is_some(),
            host.input.shown.clone(),
            view.pending
                .as_ref()
                .map(|frame| frame.subject.selected_count),
            view.drawn
                .as_ref()
                .map(|frame| frame.subject.selected_count),
        )
    });
    assert_eq!(
        read(&shell, cx, "previewColumnState", &[]),
        "multi",
        "{diagnostic:?}"
    );
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "false");
}
