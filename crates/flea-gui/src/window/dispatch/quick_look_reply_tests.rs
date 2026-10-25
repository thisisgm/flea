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
        "text"
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
