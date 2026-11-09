//! Code previews keep whole column rows and a scroll indicator; focused PDFs accept both pager directions.
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open, pump};
use crate::{
    g2::{
        doc_preview::code::CodeView,
        quick_look_overlay::{doc::View, Body},
    },
    state::Shell,
    window::{
        selection_preview::{docs::Doc, Mounted},
        u07_tests::read,
    },
};
use gpui::{
    point, px, size, Bounds, Entity, ScrollDelta, ScrollWheelEvent, TestAppContext,
    VisualTestContext,
};
use std::time::Duration;

fn code_answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, lines: &[String], slot: &str) {
    let asked = ask(lines, "code");
    let body = (0..164)
        .map(|n| format!(r#"[0,"let value_{n} = {n};"]"#))
        .collect::<Vec<_>>()
        .join(",");
    answer(
        shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"{slot}","state":"styled","first":{},"total":164,"lines":[{body}]}}"#,
            field(asked, "token"),
            field(asked, "first")
        ),
    );
    pump(cx);
}

fn column_code(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<CodeView> {
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap().read(cx);
        match &preview.drawn.as_ref().unwrap().mounted {
            Mounted::Doc(Doc::Code(code)) => code.clone(),
            _ => panic!("the column draws code"),
        }
    })
}

#[gpui::test]
fn pvcode_quick_look_wheel_reveals_then_retires_the_scroll_thumb(cx: &mut TestAppContext) {
    use super::preview_tests::{code_reply, opened, sent, token};
    let (shell, cx, mut requests) = opened(cx);
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("long.rs", "text-x-rust", "Rust source code", 0o100644)],
        1,
    );
    super::test_shell::set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    super::preview_tests::draw(cx);
    let asks = sent(&mut requests, "code");
    let text: Vec<String> = (0..164).map(|n| format!("let value_{n} = {n};")).collect();
    let lines: Vec<&str> = text.iter().map(String::as_str).collect();
    super::preview_tests::answer(&shell, cx, code_reply(token(&asks[0]), &lines, 164));
    cx.simulate_keystrokes("w");
    super::preview_tests::draw(cx);
    let Some(Body::Doc(doc)) = super::preview_tests::body(&shell, cx) else {
        panic!("Quick Look draws code")
    };
    let View::Code(code) = doc.view else {
        panic!("Quick Look draws code")
    };
    assert!(
        cx.debug_bounds("pv1-scroll-knob").is_none(),
        "no thumb at rest"
    );
    let at = cx.debug_bounds("pv1-doc").unwrap().center();
    for _ in 0..6 {
        cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., -3.)),
            ..Default::default()
        });
    }
    super::preview_tests::draw(cx);
    assert!(
        cx.read(|cx| code.read(cx).top > 0),
        "wheel reaches the mounted code view"
    );
    assert!(
        cx.debug_bounds("pv1-scroll-knob").is_some(),
        "wheel reveals the code scroll thumb"
    );
    cx.executor().advance_clock(Duration::from_millis(1001));
    super::preview_tests::draw(cx);
    assert!(
        cx.debug_bounds("pv1-scroll-knob").is_none(),
        "thumb retires after rest"
    );
    let bounds = cx.debug_bounds("pv1-doc").unwrap();
    let inset = cx.read(|cx| code.read(cx).metrics.top);
    let lane = point(bounds.right() - px(inset + 1.), bounds.center().y);
    cx.simulate_mouse_move(lane, None, gpui::Modifiers::none());
    super::preview_tests::draw(cx);
    assert!(
        cx.debug_bounds("pv1-scroll-knob").is_some(),
        "hover reveals the lane"
    );
    cx.simulate_mouse_move(point(px(30.), px(30.)), None, gpui::Modifiers::none());
    super::preview_tests::draw(cx);
    assert!(
        cx.debug_bounds("pv1-scroll-knob").is_none(),
        "leaving the code clears hover"
    );
}

// EY-428: at a Quick Look text's end the last line keeps the top pad below it, and the thumb ends where the lines do.
#[gpui::test]
fn pvcode_quick_look_end_keeps_the_pad_under_the_last_line(cx: &mut TestAppContext) {
    use super::preview_tests::{code_reply, opened, sent, token};
    let (shell, cx, mut requests) = opened(cx);
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("long.log", "text-plain", "Log", 0o100644)],
        1,
    );
    super::test_shell::set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    super::preview_tests::draw(cx);
    let asks = sent(&mut requests, "code");
    let text: Vec<String> = (0..120).map(|n| format!("line {n}")).collect();
    let lines: Vec<&str> = text.iter().map(String::as_str).collect();
    super::preview_tests::answer(&shell, cx, code_reply(token(&asks[0]), &lines, 120));
    super::preview_tests::draw(cx);
    let Some(Body::Doc(doc)) = super::preview_tests::body(&shell, cx) else {
        panic!("Quick Look draws the text")
    };
    let View::Code(code) = doc.view else {
        panic!("Quick Look draws the text")
    };
    let card = cx.debug_bounds("pv1-doc").unwrap();
    for _ in 0..60 {
        cx.simulate_event(ScrollWheelEvent {
            position: card.center(),
            delta: ScrollDelta::Lines(point(0., -3.)),
            ..Default::default()
        });
    }
    super::preview_tests::draw(cx);
    let (pad, row) = cx.read(|cx| (code.read(cx).metrics.top, code.read(cx).metrics.row));
    let last = cx.update(|window, cx| {
        code.update(cx, |code, cx| {
            let bounds = Bounds::new(point(px(0.), px(0.)), card.size);
            let frame = code.layout(bounds, window, cx);
            frame.rows.last().map(|(y, _)| *y).unwrap()
        })
    });
    let foot = f32::from(card.size.height) - pad;
    assert!(
        (pad + last + row - foot).abs() < 0.5,
        "the last line ends one pad above the card foot: {} vs {foot}",
        pad + last + row
    );
    let knob = cx.debug_bounds("pv1-scroll-knob").unwrap();
    let gap = f32::from(card.bottom() - knob.bottom()) - pad;
    assert!(
        (0. ..=3.).contains(&gap),
        "the thumb ends with the lines, {gap} px short"
    );
}

// EY-170: fitting the body font keeps the board minimum and the same pitch in both wrapping modes.
#[gpui::test]
fn pvcode_quick_look_pitch_tracks_the_body_font_before_and_after_wrapping(cx: &mut TestAppContext) {
    use super::preview_tests::{code_reply, opened, sent, token};
    let (shell, cx, mut requests) = opened(cx);
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("long.rs", "text-x-rust", "Rust source code", 0o100644)],
        1,
    );
    super::test_shell::set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    super::preview_tests::draw(cx);
    let asks = sent(&mut requests, "code");
    let line = "let measured_value = values.iter().map(|value| value + 1).collect(); ".repeat(40);
    super::preview_tests::answer(&shell, cx, code_reply(token(&asks[0]), &[&line], 1));
    for wrap in [false, true] {
        if wrap {
            cx.simulate_keystrokes("w");
        }
        super::preview_tests::draw(cx);
        let Some(Body::Doc(doc)) = super::preview_tests::body(&shell, cx) else {
            panic!("Quick Look draws code")
        };
        let View::Code(code) = doc.view else {
            panic!("Quick Look draws code")
        };
        cx.update(|window, cx| {
            let code = code.read(cx);
            assert_eq!(code.wrap, wrap, "w changes the Quick Look wrapping mode");
            assert_eq!(
                code.metrics.inset, 15.,
                "the numbered code pad starts inside the card frame, wrap={wrap}"
            );
            if wrap {
                assert!(code.rows_of(0) > 1, "the long source line visibly wraps");
            } else {
                assert_eq!(code.rows_of(0), 1, "unwrapped source stays on one row");
            }
            let run = crate::g2::doc_preview::code::shape::glyphs(
                window,
                "0",
                &code.typeface.font,
                code.typeface.body,
            );
            let pitch = f32::from(run.line().ascent + run.line().descent).ceil();
            assert_eq!(
                code.metrics.row,
                pitch.max(crate::g2::quick_look_overlay::look::TEXT_LINE),
                "the body font fits inside the board pitch, wrap={wrap}"
            );
        });
    }
}

#[gpui::test]
fn pvcode_columns_end_on_a_whole_line_at_rest_and_after_paging(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("long.rs", "text-x-rust", "Rust source code", 0o100644)],
        1,
    );
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","lines":164"#);
    code_answer(&shell, cx, &lines, "column");
    let code = column_code(&shell, cx);
    for key in [None, Some("shift-f->F"), Some("shift-b->B")] {
        if let Some(key) = key {
            cx.simulate_keystrokes(key);
            pump(cx);
        }
        cx.update(|window, cx| {
            code.update(cx, |code, cx| {
                let bounds = Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(code.frame.0), px(code.frame.1 + code.metrics.top)),
                );
                let frame = code.layout(bounds, window, cx);
                assert!(!frame.rows.is_empty(), "code is actually painted");
                assert!(
                    frame
                        .rows
                        .iter()
                        .all(|(y, _)| *y + code.metrics.row <= code.frame.1),
                    "column paints no clipped row after {key:?}"
                );
            })
        });
    }
}

#[gpui::test]
fn pvcode_focused_column_pdf_h_and_left_return_to_page_one(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_threads(|| pdf_keys(cx));
}

fn pdf_keys(cx: &mut TestAppContext) {
    use crate::g2::doc_preview::{render::Reply, surface::Surface};
    let (shell, cx) = open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file(
            "pages.pdf",
            "application-pdf",
            "PDF document",
            0o100644,
        )],
        1,
    );
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","pages":3"#);
    let pdf = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap();
        let preview = preview.read(cx);
        let frame = preview.pending.as_ref().or(preview.drawn.as_ref()).unwrap();
        match &frame.mounted {
            Mounted::Pdf(pdf) => pdf.clone(),
            _ => panic!("PDF mounted"),
        }
    });
    cx.update(|_, cx| {
        pdf.update(cx, |pdf, cx| {
            let mut surface = Surface::new(true, 66, 100);
            surface.set_active(true, 0);
            surface.set_path("/files/pages.pdf", 0);
            surface.frame();
            surface.opening(1);
            surface.set_frame(pdf.frame.0.max(274.), pdf.frame.1.max(560.), 1., 9.);
            surface.reply(
                Reply::Opened {
                    doc: 1,
                    sizes: vec![(612., 792.); 3],
                    read: 3,
                },
                0,
            );
            for page in 0..3 {
                let image =
                    std::sync::Arc::new(gpui::RenderImage::from_bgra(1, 1, vec![255; 4]).unwrap());
                surface.reply(
                    Reply::Rendered {
                        doc: 1,
                        page,
                        width: surface.render_width,
                        image,
                        took: Duration::ZERO,
                    },
                    0,
                );
            }
            pdf.surface = surface;
            cx.notify();
        })
    });
    pump(cx);
    assert_eq!(read(&shell, cx, "columnPdfPages", &[]), "3");
    cx.simulate_keystrokes("ctrl-tab");
    for (forward, back) in [("l", "h"), ("right", "left")] {
        cx.simulate_keystrokes(forward);
        pump(cx);
        assert_eq!(
            read(&shell, cx, "columnPdfPage", &[]),
            "1",
            "{forward} shows page two"
        );
        cx.simulate_keystrokes(back);
        pump(cx);
        assert_eq!(
            read(&shell, cx, "columnPdfPage", &[]),
            "0",
            "{back} returns to page one"
        );
    }
}
