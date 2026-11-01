//! The card accepts only its pending font sample or PDF copy, leaving another surface's replies untouched.
use super::preview_tests::{body, close, draw, file, opened, sent};
use super::test_shell::{land, set_cursor};
use crate::{
    backend::Event,
    g2::pv1::{doc::View, Body},
    state::Shell,
};
use flea::ui_logic_pv2::{font::next_look_id, pdf::Fetch};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[path = "quick_look_hold_tests.rs"]
mod hold;

#[gpui::test]
fn a_drained_backend_fails_the_mounted_code_read(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(sent(&mut requests, "code").len(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.receive(0, Event::Drained, window, cx))
    });
    draw(cx);
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "previewOpen", &[]),
        "true"
    );
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "previewState", &[]),
        "This file could not be read."
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_drained_backend_preserves_an_already_loaded_code_document(cx: &mut TestAppContext) {
    use super::preview_tests::{answer, code_reply, token};
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let asks = sent(&mut requests, "code");
    answer(&shell, cx, code_reply(token(&asks[0]), &["complete"], 1));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.receive(0, Event::Drained, window, cx))
    });
    draw(cx);
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "previewState", &[]),
        "ready"
    );
    // EY-447: previewText reads the card's own lines, as IpcPreviewState's look.textShown does.
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "previewText", &[]),
        "complete"
    );
    close(&shell, cx);
}

fn route(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: String) -> bool {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.quick_look_line(0, &Event::Producer(line), window, cx)
        })
    })
}

#[gpui::test]
fn font_mount_sends_its_first_ask_before_layout(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("Inter.ttf", "font-x-generic")],
        1,
    );
    set_cursor(&shell, cx, 0);
    std::iter::from_fn(&mut requests).for_each(drop);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.open_quick_look(0, window, cx));
        })
    });
    let asks = sent(&mut requests, "fontlook");
    assert_eq!(asks.len(), 1, "mount asks without waiting for layout");
    assert_eq!(
        asks[0].get("path").and_then(flea::jsondoc::Json::as_str),
        Some("/files/Inter.ttf")
    );
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("font mounted")
    };
    let View::Font(font) = doc.view else {
        panic!("font view")
    };
    assert_eq!(
        asks[0].get("id").and_then(flea::jsondoc::Json::as_f64),
        Some(cx.read(|cx| font.read(cx).page.pending) as f64)
    );
    close(&shell, cx);
}

#[gpui::test]
fn an_unrevealed_card_does_not_accept_an_invisible_close_click(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("Inter.ttf", "font-x-generic")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert!(!shell.read_with(cx, |shell, cx| shell
        .quick_look
        .as_ref()
        .unwrap()
        .view
        .read(cx)
        .is_revealed()));
    let at = cx
        .debug_bounds("pv1-close")
        .expect("invisible control lays out")
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    draw(cx);
    assert!(shell.read_with(cx, |shell, cx| shell
        .quick_look
        .as_ref()
        .unwrap()
        .view
        .read(cx)
        .is_open()));
    let at = cx
        .debug_bounds("pv1-close")
        .expect("revealed control")
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    draw(cx);
    assert!(
        !shell.read_with(cx, |shell, cx| shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .read(cx)
            .is_open()),
        "a subsequent visible click closes normally"
    );
    close(&shell, cx);
}

#[gpui::test]
fn another_font_surface_keeps_its_own_reply(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("Inter.ttf", "font-x-generic")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("font mounted")
    };
    let View::Font(font) = doc.view else {
        panic!("font view")
    };
    let pending = cx.read(|cx| font.read(cx).page.pending);
    assert_ne!(pending, 0);
    let other = next_look_id();
    assert!(
        !route(
            &shell,
            cx,
            format!(r#"{{"t":"fontlooked","id":{other},"err":"column reply"}}"#)
        ),
        "the column's reply must reach its router"
    );
    assert!(!cx.read(|cx| font.read(cx).page.failed()));
    assert!(route(
        &shell,
        cx,
        format!(r#"{{"t":"fontlooked","id":{pending},"err":"card reply"}}"#)
    ));
    assert!(cx.read(|cx| font.read(cx).page.failed()));
    close(&shell, cx);
}

#[gpui::test]
fn another_pdf_surface_keeps_its_own_copy_reply(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("book.pdf", "application-pdf")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("PDF mounted")
    };
    let View::Pdf(pdf) = doc.view else {
        panic!("PDF view")
    };
    pdf.update(cx, |pdf, _| pdf.surface.fetch = Fetch::Waiting(71));
    assert!(!route(
        &shell,
        cx,
        r#"{"t":"pdfcopied","id":72,"err":"column reply"}"#.into()
    ));
    assert_eq!(
        cx.read(|cx| pdf.read(cx).surface.fetch.clone()),
        Fetch::Waiting(71)
    );
    assert!(route(
        &shell,
        cx,
        r#"{"t":"pdfcopied","id":71,"err":"card reply"}"#.into()
    ));
    assert_eq!(
        cx.read(|cx| pdf.read(cx).surface.fetch.clone()),
        Fetch::Failed { stalled: false }
    );
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_video_strip_opens_rearms_and_hides_after_contact(cx: &mut TestAppContext) {
    use super::quick_look_body_tests::{mounted, open_kind};
    use crate::window::u07_tests::read;
    use flea::ui_logic_pv3::frame::Event;
    use std::time::Duration;

    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    for event in [
        Event::Loaded {
            video: true,
            duration_ms: 20_000,
        },
        Event::Playing,
    ] {
        assert!(script.feeds.borrow()[0].event(event));
    }
    draw(cx);
    let visible = |cx: &mut VisualTestContext| {
        read(&shell, cx, "previewStrip", &[]).starts_with(r#"{"visible":true,"#)
    };
    assert!(visible(cx), "a playing video's strip starts visible");
    cx.executor().advance_clock(Duration::from_secs(3));
    draw(cx);
    let point = read(&shell, cx, "previewSliderCentre", &[])
        .split_whitespace()
        .map(|value| value.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    cx.simulate_mouse_move(
        gpui::point(gpui::px(point[0]), gpui::px(point[1])),
        None,
        gpui::Modifiers::none(),
    );
    draw(cx);
    cx.executor().advance_clock(Duration::from_secs(3));
    draw(cx);
    assert!(visible(cx), "contact restarts the four-second interval");
    cx.executor().advance_clock(Duration::from_secs(2));
    draw(cx);
    assert!(!visible(cx), "an idle playing video hides its strip");
    assert!(script.feeds.borrow()[0].event(Event::Paused));
    draw(cx);
    assert!(visible(cx), "a paused video keeps its controls visible");
    cx.executor().advance_clock(Duration::from_secs(5));
    draw(cx);
    assert!(visible(cx));
    close(&shell, cx);
}

#[gpui::test]
fn quick_look_pdf_state_reads_painted_controls_and_surface(cx: &mut TestAppContext) {
    use super::quick_look_body_tests::{mounted, open_kind};
    use crate::window::u07_tests::{read, reply};
    use flea::jsondoc;

    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "book.pdf");
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("PDF mounted")
    };
    let View::Pdf(pdf) = doc.view else {
        panic!("PDF view")
    };
    pdf.update(cx, |pdf, cx| {
        pdf.surface.sizes = vec![(612., 792.); 3];
        cx.notify();
    });
    draw(cx);
    let state = reply(&shell, cx, "pdfState", &["true"]);
    assert_eq!(state.code, 0, "{}", state.stderr);
    let state = jsondoc::parse(&state.stdout).unwrap();
    assert_eq!(state.get("pages").and_then(jsondoc::Json::as_f64), Some(3.));
    assert_eq!(
        state.get("control").and_then(jsondoc::Json::as_f64),
        Some(-1.)
    );
    let controls = state
        .get("controls")
        .and_then(jsondoc::Json::as_array)
        .unwrap();
    assert_eq!(controls.len(), 6);
    assert_eq!(
        controls[0].get("enabled").and_then(jsondoc::Json::as_bool),
        Some(false)
    );
    let centre = controls[1]
        .get("centre")
        .and_then(jsondoc::Json::as_str)
        .unwrap();
    let expected = cx.debug_bounds("pv1-strip-1").unwrap().center();
    assert_eq!(
        centre,
        format!(
            "{} {}",
            f32::from(expected.x).round(),
            f32::from(expected.y).round()
        )
    );
    assert!(!read(&shell, cx, "previewSurfaceRect", &[]).is_empty());
    cx.simulate_keystrokes("tab");
    draw(cx);
    let state = jsondoc::parse(&read(&shell, cx, "pdfState", &["true"])).unwrap();
    assert_eq!(
        state.get("control").and_then(jsondoc::Json::as_f64),
        Some(1.)
    );
    assert_eq!(
        state
            .get("controls")
            .and_then(jsondoc::Json::as_array)
            .unwrap()[1]
            .get("ring")
            .and_then(jsondoc::Json::as_bool),
        Some(true)
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "pdfState", &["true"]), "null");
    assert_eq!(read(&shell, cx, "previewSurfaceRect", &[]), "");
    close(&shell, cx);
}
