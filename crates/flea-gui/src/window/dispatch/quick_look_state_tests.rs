//! Quick Look's Markdown, PDF, media and archive readers answer the mounted card's own live state.
use super::{
    preview_tests::{body, close, draw},
    quick_look_body_tests::{mounted, open_kind},
};
use crate::{
    g2::{
        pv1::{doc::View, Body},
        pv2::markdown::{Status, View as Shown},
    },
    state::Shell,
    window::u07_tests::read,
};
use flea::{ui_logic_pv2::archive::Meta, ui_logic_pv3::frame::Event};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn mounted_view(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> View {
    let Some(Body::Doc(doc)) = body(shell, cx) else {
        panic!("a document is mounted")
    };
    doc.view
}

#[gpui::test]
fn quick_look_state_duration_reads_the_player_length(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "song.ogg");
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: false,
        duration_ms: 20_000,
    }));
    draw(cx);
    assert_eq!(read(&shell, cx, "previewDuration", &[]), "20000");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewDuration", &[]),
        "0",
        "a closed card has no length"
    );
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_state_markdown_reads_view_text_length_and_end_gap(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "readme.md");
    let View::Markdown(markdown) = mounted_view(&shell, cx) else {
        panic!("Markdown view")
    };
    assert_eq!(read(&shell, cx, "previewMarkdownView", &[]), "rendered");
    cx.simulate_keystrokes("r");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewMarkdownView", &[]),
        "source",
        "r flips the shown view and the reader follows it"
    );
    markdown.update(cx, |view, _| {
        view.doc.text = Some("a\u{1F600}".into());
    });
    assert_eq!(
        read(&shell, cx, "previewTextLength", &[]),
        "3",
        "the length counts UTF-16 code units, not bytes or characters"
    );
    assert_eq!(
        read(&shell, cx, "previewEndGap", &[]),
        "-1",
        "Source answers -1 until its own end gap is measured"
    );
    cx.simulate_keystrokes("r");
    draw(cx);
    markdown.update(cx, |view, cx| {
        view.doc.status = Status::Ready;
        view.doc.complete = true;
        view.doc.height = 4000.;
        view.frame = (800., 200.);
        view.set_view(Shown::Rendered, cx);
    });
    let full = cx.read(|cx| markdown.read(cx).content_height());
    assert_eq!(
        read(&shell, cx, "previewEndGap", &[]),
        ((full - 200.).round() as i64).to_string(),
        "the end gap is the document below the viewport's bottom edge"
    );
    markdown.update(cx, |view, cx| view.scroll_by(27.5, cx));
    let (scroll, full, viewport) = cx.read(|cx| {
        let view = markdown.read(cx);
        (view.scroll, view.content_height(), view.frame.1)
    });
    assert_eq!(scroll, 27.5, "the scroll lands where it was asked");
    assert_eq!(
        read(&shell, cx, "previewEndGap", &[]),
        ((full - scroll - viewport).round() as i64).to_string(),
        "the end gap follows the live scroll, document height and viewport height"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewMarkdownView", &[]), "");
    assert_eq!(read(&shell, cx, "previewEndGap", &[]), "-1");
    assert_eq!(read(&shell, cx, "previewTextLength", &[]), "0");
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_state_pdf_reads_page_zoom_focus_and_expanded(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "book.pdf");
    assert_eq!(read(&shell, cx, "previewPdfPage", &[]), "0");
    assert_eq!(read(&shell, cx, "previewPdfFocus", &[]), "-1");
    assert_eq!(read(&shell, cx, "previewExpanded", &[]), "false");
    let View::Pdf(pdf) = mounted_view(&shell, cx) else {
        panic!("PDF view")
    };
    pdf.update(cx, |pdf, _| {
        pdf.surface.sizes = vec![(612., 792.); 3];
        pdf.surface.page = 2;
        let viewer = pdf.viewer.as_mut().expect("Quick Look's viewer");
        viewer.zoom = 1.25;
        viewer.expanded = true;
        viewer.control = Some(1);
    });
    draw(cx);
    assert_eq!(read(&shell, cx, "previewPdfPage", &[]), "2");
    assert_eq!(read(&shell, cx, "previewPdfZoom", &[]), "1.25");
    assert_eq!(read(&shell, cx, "previewPdfFocus", &[]), "1");
    assert_eq!(read(&shell, cx, "previewExpanded", &[]), "true");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewPdfPage", &[]),
        "-1",
        "a closed card has no PDF"
    );
    assert_eq!(read(&shell, cx, "previewExpanded", &[]), "");
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_state_archive_reads_the_mounted_index_names(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "pack.zip");
    assert_eq!(
        read(&shell, cx, "previewArchiveNames", &[]),
        "",
        "no index has landed yet"
    );
    let View::Archive(archive) = mounted_view(&shell, cx) else {
        panic!("archive view")
    };
    archive.update(cx, |archive, _| {
        archive.meta = Some(Meta {
            entries: 2,
            unpacked: 3,
            failed: false,
            names: vec![("a.txt".into(), false), ("docs".into(), true)],
        });
    });
    draw(cx);
    assert_eq!(read(&shell, cx, "previewArchiveNames", &[]), "a.txt|docs");
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_state_media_strip_reads_the_painted_seek_and_mute_centres(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "song.ogg");
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: false,
        duration_ms: 20_000,
    }));
    draw(cx);
    let seek = read(&shell, cx, "previewSliderCentre", &[]);
    assert!(
        seek.split(' ').count() == 2 && seek.split(' ').all(|value| value.parse::<f32>().is_ok()),
        "the seek track's centre is painted as \"x y\": {seek:?}"
    );
    let strip = read(&shell, cx, "previewStrip", &[]);
    assert!(
        strip.starts_with(r#"{"visible":true,"muted":false,"mute":""#)
            && !strip.contains(r#""mute":"""#),
        "the strip reports visible, unmuted and the mute mark's centre: {strip}"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewSliderCentre", &[]),
        "",
        "a closed card has no seek track"
    );
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_state_video_strip_retires_hidden_paint_geometry(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    }));
    draw(cx);
    let paused = read(&shell, cx, "previewStrip", &[]);
    assert!(paused.starts_with(r#"{"visible":true,"muted":false,"mute":""#));
    assert!(!paused.contains(r#""mute":"""#));
    assert!(script.feeds.borrow()[0].event(Event::Playing));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(4));
    draw(cx);
    let hidden = read(&shell, cx, "previewStrip", &[]);
    assert!(
        hidden.starts_with(r#"{"visible":false,"muted":false,"mute":""}"#),
        "a hidden video strip has no visible paint or mute target: {hidden}"
    );
    assert_eq!(read(&shell, cx, "previewSliderCentre", &[]), "");
    assert!(script.feeds.borrow()[0].event(Event::Paused));
    draw(cx);
    let paused_again = read(&shell, cx, "previewStrip", &[]);
    assert!(paused_again.starts_with(r#"{"visible":true,"muted":false,"mute":""#));
    assert!(!paused_again.contains(r#""mute":"""#));
    close(&shell, cx);
}

// EY-238: every reader the sa11 preview scenes ask answers a value on each Quick Look kind and on the closed card, as IpcPreviewState.qml does.
#[gpui::test]
fn ey238_preview_readers_answer_on_every_kind(cx: &mut TestAppContext) {
    const READERS: &[&str] = &[
        "previewDuration",
        "previewEndGap",
        "previewSelectionState",
        "previewExpanded",
        "previewPdfPage",
        "previewPdfFocus",
        "previewPdfZoom",
        "previewSliderCentre",
        "previewMarkdownView",
        "previewFigures",
        "previewTextLength",
        "previewCloseState",
        "previewStrip",
        "previewScrollY",
        "columnMarkdownView",
        "previewArchiveNames",
    ];
    let (shell, cx, _requests, _script) = mounted(cx);
    let mut unanswered = Vec::new();
    for name in [
        "",
        "notes.txt",
        "main.rs",
        "readme.md",
        "clip.mp4",
        "book.pdf",
        "pack.zip",
    ] {
        if !name.is_empty() {
            open_kind(&shell, cx, name);
            let open = shell.read_with(cx, |shell, cx| {
                let look = shell.quick_look.as_ref()?.view.read(cx);
                look.subject().map(|subject| subject.name.clone())
            });
            assert_eq!(open.as_deref(), Some(name), "{name} opens its card");
        }
        for reader in READERS {
            cx.run_until_parked();
            let reply = crate::window::u07_tests::reply(&shell, cx, reader, &[]);
            if reply.code != 0 {
                unanswered.push(format!("{name}:{reader}:{}", reply.stderr.trim()));
            }
        }
        if !name.is_empty() {
            cx.simulate_keystrokes("escape");
            draw(cx);
        }
    }
    assert!(unanswered.is_empty(), "unanswered readers: {unanswered:#?}");
    assert_eq!(
        read(&shell, cx, "previewStrip", &[]),
        r#"{"visible":false,"muted":false,"mute":""}"#,
        "a closed card draws no strip"
    );
    assert_eq!(read(&shell, cx, "previewTextLength", &[]), "0");
    close(&shell, cx);
}

// EY-238: text and code, including HTML Source, count UTF-16 units from owned text or joined backend lines.
#[gpui::test]
fn ey238_preview_text_length_counts_utf16_on_text_code_and_html(cx: &mut TestAppContext) {
    use flea::ui_logic_pv2::{
        code_view::{Answer, Lines, Reply},
        html::View as HtmlView,
    };
    let (shell, cx, _requests, _script) = mounted(cx);
    for name in ["notes.txt", "main.rs", "page.html"] {
        open_kind(&shell, cx, name);
        let code = match mounted_view(&shell, cx) {
            View::Code(code) => code,
            View::Html(html) => {
                html.update(cx, |view, cx| view.set_view(HtmlView::Source, cx));
                cx.read(|cx| {
                    html.read(cx)
                        .source
                        .clone()
                        .expect("HTML Source mounts code")
                })
            }
            _ => panic!("{name} mounts text or HTML Source"),
        };
        code.update(cx, |view, _| {
            view.lines.text = Some("a\u{1f600}\n\u{03b2}".into());
        });
        assert_eq!(
            read(&shell, cx, "previewTextLength", &[]),
            "5",
            "{name}: owned text counts surrogate pairs and newlines"
        );
        code.update(cx, |view, _| {
            view.lines = Lines::new(0);
            view.lines.sent(9);
            assert!(view.lines.answered(Reply {
                token: 9,
                state: Answer::Styled,
                first: 1,
                total: 2,
                lines: vec![vec![(0, "a\u{1f600}".into())], vec![(0, "\u{03b2}".into())]],
            }));
        });
        assert_eq!(
            read(&shell, cx, "previewTextLength", &[]),
            "5",
            "{name}: held lines include their joining newline"
        );
        code.update(cx, |view, _| view.lines.text = Some(String::new()));
        assert_eq!(read(&shell, cx, "previewTextLength", &[]), "0");
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}

// EY-238: previewFigures names each top-level figure block of the open Markdown card instead of answering unported.
#[gpui::test]
fn ey238_preview_figures_reads_the_open_markdown_figures(cx: &mut TestAppContext) {
    use flea::ui_logic_pv2::markdown::{
        markdown_layout::{Counter, FigureState},
        Block, Figure, FigureKind,
    };
    use std::sync::Arc;
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "readme.md");
    let View::Markdown(markdown) = mounted_view(&shell, cx) else {
        panic!("readme.md mounts the Markdown view")
    };
    let figure = Block::Figure(Figure {
        kind: FigureKind::Mermaid,
        source: "graph TD; A-->B".into(),
        display: true,
    });
    let run = Block::Run {
        text: "text".into(),
        maths: Vec::new(),
    };
    let counter = |figure| Counter {
        figure,
        ..Counter::default()
    };
    markdown.update(cx, |view, _| {
        view.doc.status = Status::Ready;
        view.doc.complete = true;
        view.doc.parsed.blocks = Arc::new(vec![run, figure.clone(), figure]);
        view.doc.parsed.prefix = Arc::new(vec![counter(0), counter(0), counter(1), counter(2)]);
        let mut assets = (*view.doc.assets).clone();
        assets.figures.insert(1, FigureState::Failed);
        view.doc.assets = Arc::new(assets);
    });
    assert_eq!(
        read(&shell, cx, "previewFigures", &[]),
        "1=deferred,2=failed",
        "each figure block reads by its index, with the state its drawing reached"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewFigures", &[]), "");
    close(&shell, cx);
}

#[path = "quick_look_state_quality_tests.rs"]
mod quality;
