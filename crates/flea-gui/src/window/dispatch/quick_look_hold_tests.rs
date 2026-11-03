//! Quick Look waits for drawable content and routes input through its mounted document.
use super::super::{
    preview_tests::{close, draw, file, opened, sent, token},
    test_shell::{land, set_cursor},
};
use crate::{backend::Event, window::u07_tests::read};
use gpui::TestAppContext;

#[gpui::test]
fn quick_look_picture_metadata_keeps_the_card_held_until_raster_completion(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("picture.png", "image-png")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let asked = token(&sent(&mut requests, "meta")[0]);
    let line = format!(
        r#"{{"t":"meta","row":0,"w":800,"h":600,"orient":1,"ms":0,"rate":0,"entries":0,"unpacked":0,"afailed":false,"names":[],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm","token":{asked}}}"#
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Event::Producer(line), window, cx);
            assert!(
                !shell
                    .quick_look
                    .as_ref()
                    .unwrap()
                    .view
                    .read(cx)
                    .is_revealed(),
                "metadata alone cannot reveal the picture"
            );
        });
    });
    let state = read(&shell, cx, "previewSwapState", &[]);
    assert!(state.contains(r#""revealedEarly":false"#), "{state}");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewState", &[]),
        "image",
        "the terminal decode refusal can draw"
    );
    let state = read(&shell, cx, "previewSwapState", &[]);
    assert!(state.contains(r#""revealedEarly":false"#), "{state}");
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_ended_video_picture_rect_uses_the_empty_surface_layout(cx: &mut TestAppContext) {
    use super::super::quick_look_body_tests::{mounted, open_kind};
    use flea::ui_logic_pv3::frame::Event;
    use std::sync::Arc;

    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    let picture = Arc::new(gpui::RenderImage::from_bgra(4, 2, vec![128; 32]).unwrap());
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: true,
        duration_ms: 2_000
    }));
    assert!(script.feeds.borrow()[0].frame(picture.clone()));
    assert!(script.feeds.borrow()[0].event(Event::Playing));
    draw(cx);
    let surface = read(&shell, cx, "previewSurfaceRect", &[]);
    assert!(!surface.is_empty());
    assert_ne!(
        read(&shell, cx, "previewPictureRect", &[]),
        surface,
        "playing video is letterboxed"
    );
    for event in [Event::Ended, Event::Stopped, Event::Position(0)] {
        assert!(script.feeds.borrow()[0].event(event));
    }
    assert!(script.feeds.borrow()[0].frame(picture));
    draw(cx);
    assert_eq!(read(&shell, cx, "previewState", &[]), "stopped");
    assert_eq!(
        read(&shell, cx, "previewPictureRect", &[]),
        read(&shell, cx, "previewSurfaceRect", &[]),
        "the native oracle's empty VideoOutput retains its surface layout bounds"
    );
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_immediate_escape_space_keeps_the_new_subject_and_pending_reply(
    cx: &mut TestAppContext,
) {
    use gpui::{KeyDownEvent, Keystroke, Modifiers};
    let (shell, cx, _requests) = opened(cx);
    land(&shell, cx, "/files", vec![file("slow.png", "image-png")], 1);
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::CAP_MS));
    draw(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for key in ["escape", "space"] {
                shell.key(
                    &KeyDownEvent {
                        keystroke: Keystroke {
                            key: key.into(),
                            key_char: None,
                            modifiers: Modifiers::none(),
                            is_keypad: false,
                            us_key: None,
                        },
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
            }
            assert!(
                shell.quick_look.as_ref().unwrap().view.read(cx).is_open(),
                "Space reopens synchronously"
            );
        });
    });
    draw(cx);
    shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap();
        assert!(
            look.view.read(cx).is_open(),
            "the new subject survives old close delivery"
        );
        assert_eq!(
            look.row,
            Some(0),
            "old close delivery cannot retire the new metadata reply"
        );
        assert_ne!(look.token, 0, "the new metadata request stays live");
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::CAP_MS));
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    cx.simulate_keystrokes("escape space");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "real window dispatch also reopens inside the fade"
    );
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_pdf_cursor_keys_pan_the_zoomed_page_and_keep_the_listing_cursor(
    cx: &mut TestAppContext,
) {
    use super::super::{
        preview_tests::body,
        quick_look_body_tests::{mounted, open_kind},
    };
    use crate::g2::{
        pv1::{doc::View, Body},
        pv2::render,
    };

    render::without_threads(|| {
        let (shell, cx, _requests, _script) = mounted(cx);
        open_kind(&shell, cx, "book.pdf");
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("PDF mounted")
        };
        let View::Pdf(pdf) = doc.view else {
            panic!("PDF view")
        };
        pdf.update(cx, |pdf, cx| {
            pdf.surface.sizes = vec![(440., 1100.); 3];
            pdf.surface.page = 1;
            let viewer = pdf.viewer.as_mut().unwrap();
            viewer.zoom = 1.25;
            viewer.control = Some(5);
            cx.notify();
        });
        draw(cx);
        assert!(
            cx.read(|cx| pdf.read(cx).frame.1 > 0.),
            "the PDF has a laid-out viewport"
        );
        let cursor = read(&shell, cx, "cursor", &[]);
        cx.simulate_keystrokes("down");
        draw(cx);
        assert!(
            cx.read(|cx| pdf.read(cx).pan.1 > 0.),
            "Down scrolls the zoomed PDF instead of following the listing"
        );
        assert_eq!(read(&shell, cx, "cursor", &[]), cursor);
        assert_eq!(read(&shell, cx, "previewPdfPage", &[]), "1");
        assert_eq!(read(&shell, cx, "previewPdfFocus", &[]), "5");
        cx.simulate_keystrokes("up");
        draw(cx);
        assert_eq!(cx.read(|cx| pdf.read(cx).pan.1), 0.);
        assert_eq!(read(&shell, cx, "cursor", &[]), cursor);
        cx.simulate_keystrokes("ctrl-tab");
        draw(cx);
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "false",
            "Ctrl+Tab exits the PDF viewer"
        );
        close(&shell, cx);
    });
}

#[gpui::test]
fn quick_look_column_pdf_expands_collapses_and_returns_keys_to_the_listing(
    cx: &mut TestAppContext,
) {
    use super::super::previewtypes_tests as host;
    use crate::{g2::pv2::render, window::selection_preview::Mounted};
    render::without_threads(|| {
        let (shell, cx) = host::open(cx, "/files");
        land(
            &shell,
            cx,
            "/files",
            vec![
                host::file("book.pdf", "application-pdf", "PDF", 0o100644),
                host::file("next.txt", "text-x-generic", "Plain text", 0o100644),
            ],
            2,
        );
        let lines = host::local(&shell, cx, "/files");
        host::meta(&shell, cx, &lines, "");
        host::pump(cx);
        let pdf = shell.read_with(cx, |shell, cx| {
            let view = shell.selection_preview(0).unwrap().read(cx);
            let Mounted::Pdf(pdf) = &view.pending.as_ref().expect("column pending").mounted else {
                panic!("column PDF mounted")
            };
            pdf.clone()
        });
        pdf.update(cx, |pdf, cx| {
            let surface = &mut pdf.surface;
            surface.set_active(true, 0);
            surface.opening(1);
            surface.set_frame(440., 700., 1., 8.);
            surface.reply(
                render::Reply::Opened {
                    doc: 1,
                    read: 3,
                    sizes: vec![(440., 1100.); 3],
                },
                0,
            );
            surface.reply(
                render::Reply::Rendered {
                    doc: 1,
                    page: 0,
                    width: 440,
                    image: std::sync::Arc::new(
                        gpui::RenderImage::from_bgra(4, 4, vec![255; 64]).unwrap(),
                    ),
                    took: std::time::Duration::ZERO,
                },
                0,
            );
            assert_eq!(
                surface.shown_page(),
                Some(0),
                "a rendered page releases the column hold"
            );
            cx.notify();
        });
        host::pump(cx);
        shell.read_with(cx, |shell, cx| {
            assert!(
                shell.selection_preview(0).unwrap().read(cx).drawn.is_some(),
                "column PDF drawn"
            );
        });
        cx.simulate_keystrokes("ctrl-tab e");
        draw(cx);
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
        assert_eq!(
            read(&shell, cx, "previewExpanded", &[]),
            "true",
            "e expands the PDF that opens from the column"
        );
        shell.read_with(cx, |shell, cx| {
            assert!(
                shell
                    .quick_look
                    .as_ref()
                    .unwrap()
                    .view
                    .read(cx)
                    .is_expanded(),
                "the painted card agrees with its PDF viewer"
            );
        });
        cx.simulate_keystrokes("e");
        draw(cx);
        assert_eq!(read(&shell, cx, "previewExpanded", &[]), "false");
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
        assert!(
            cx.update(|window, cx| shell.read_with(cx, |shell, cx| !shell
                .selection_preview(0)
                .unwrap()
                .read(cx)
                .focus
                .is_focused(window))),
            "Escape after expansion returns listing focus"
        );
        cx.update(|window, cx| {
            shell.read_with(cx, |shell, _| {
                assert!(shell.focus.is_focused(window), "the listing owns focus");
                assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
            });
        });
        assert_eq!(read(&shell, cx, "cursor", &[]), "0");
        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(
            read(&shell, cx, "cursor", &[]),
            "1",
            "Down moves the listing"
        );
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
        close(&shell, cx);
    });
}
