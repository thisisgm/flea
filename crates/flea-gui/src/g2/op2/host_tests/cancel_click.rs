//! A pointer click on a card's button presses it as Enter on it would, through the production window's pointer path.
use super::*;
use gpui::{point, px, Bounds, Modifiers, MouseButton, Pixels, Point};

fn click(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_move(at, None, Modifiers::none());
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx, handle);
}

fn drawn(cx: &mut TestAppContext, handle: Handle, selector: &'static str) -> Bounds<Pixels> {
    VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

fn card_open(cx: &mut TestAppContext, handle: Handle) -> bool {
    edit(cx, handle, |shell, _, _| {
        shell.operations.archive.card_open()
    })
}

/// Whether the open card draws the button at this index with the label behind this catalog key.
fn button_is(cx: &mut TestAppContext, handle: Handle, at: usize, key: &'static str) -> bool {
    edit(cx, handle, move |shell, _, _| {
        let catalog = shell.locale.catalog.clone();
        let archive = &shell.operations.archive;
        let card = match archive.compress.as_ref().filter(|card| card.opened()) {
            Some(card) => card.card(&catalog),
            None => archive.extract.card(&catalog),
        };
        card.buttons[at].label == catalog.tr(key)
    })
}

fn password_len(cx: &mut TestAppContext, handle: Handle) -> usize {
    edit(cx, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .compress
            .as_ref()
            .map_or(0, |card| card.password_len())
    })
}

fn archive_lines(requests: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    drain(requests)
        .into_iter()
        .filter(|line| line.contains(r#""c":"archive""#))
        .collect()
}

fn compress_card(
    cx: &mut TestAppContext,
) -> (
    Handle,
    impl FnMut() -> Option<String>,
    impl FnMut() -> Option<String>,
) {
    let (handle, mut requests, other) = start(cx);
    choose(
        cx,
        handle,
        72,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    assert!(
        card_open(cx, handle),
        "With Password opens the Compress card"
    );
    drain(&mut requests);
    (handle, requests, other)
}

fn extract_card(
    cx: &mut TestAppContext,
) -> (
    Handle,
    impl FnMut() -> Option<String>,
    impl FnMut() -> Option<String>,
) {
    let (handle, mut requests, other) = start(cx);
    choose(cx, handle, 43, "extract", "", &["/files/photos.7z"]);
    receive(cx, handle, 0, r#"{"t":"archivestarted","id":7}"#);
    receive(cx, handle, 0, r#"{"t":"extractstarted","id":7}"#);
    receive(
        cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.7z","dest":"/files/photos"}"#,
    );
    assert!(
        card_open(cx, handle),
        "the encrypted answer opens the password card"
    );
    drain(&mut requests);
    (handle, requests, other)
}

#[test]
fn b02_a_click_on_compress_cancel_closes_the_card_and_sends_nothing() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, mut other) = compress_card(&mut cx);
    keys(&mut cx, handle, "s->s e->e c->c");
    assert_eq!(password_len(&mut cx, handle), 3, "the secret is typed");
    assert!(button_is(&mut cx, handle, 0, "archive.cancel"));
    let cancel = drawn(&mut cx, handle, "op2-button-0");
    click(&mut cx, handle, cancel.center());
    assert!(
        !card_open(&mut cx, handle),
        "Cancel closes the card, as Escape does"
    );
    assert!(
        archive_lines(&mut requests).is_empty(),
        "nothing is compressed"
    );
    assert!(archive_lines(&mut other).is_empty());
    assert_eq!(
        password_len(&mut cx, handle),
        0,
        "the typed secret leaves with the card"
    );
}

#[test]
fn b02_a_click_on_extract_cancel_closes_the_card_and_sends_nothing() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, mut other) = extract_card(&mut cx);
    keys(&mut cx, handle, "s->s e->e c->c");
    assert!(button_is(&mut cx, handle, 0, "archive.cancel"));
    let cancel = drawn(&mut cx, handle, "op2-button-0");
    click(&mut cx, handle, cancel.center());
    assert!(
        !card_open(&mut cx, handle),
        "Cancel closes the card, as Escape does"
    );
    assert!(
        archive_lines(&mut requests).is_empty(),
        "nothing is extracted"
    );
    assert!(archive_lines(&mut other).is_empty());
}

#[test]
fn b02_a_press_on_the_card_keeps_it_and_a_press_on_the_scrim_cancels() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = compress_card(&mut cx);
    let card = drawn(&mut cx, handle, "op2-card");
    click(&mut cx, handle, card.origin + point(px(12.), px(12.)));
    assert!(
        card_open(&mut cx, handle),
        "a press on the card's own ground is the card's"
    );
    click(&mut cx, handle, point(px(4.), px(4.)));
    assert!(
        !card_open(&mut cx, handle),
        "a press on the scrim answers as Cancel"
    );
    assert!(archive_lines(&mut requests).is_empty());
}

#[test]
fn b02_the_eye_still_reveals_while_held_under_the_card_frame() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests, _other) = compress_card(&mut cx);
    keys(&mut cx, handle, "s->s e->e c->c");
    let eye = edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.eye.get()
    })
    .expect("the Password eye painted");
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_move(eye.center(), None, Modifiers::none());
    visual.simulate_mouse_down(eye.center(), MouseButton::Left, Modifiers::none());
    frame(&mut cx, handle);
    assert!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .compress
            .as_ref()
            .is_some_and(|card| card.eye_held())),
        "a held eye reveals its field"
    );
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_up(eye.center(), MouseButton::Left, Modifiers::none());
    frame(&mut cx, handle);
    assert!(
        card_open(&mut cx, handle),
        "the eye's press never answers as Cancel"
    );
}

/// The archive lines one Compress submission sends, pressed by a click on the button or by Enter on it.
fn submitted(by_click: bool) -> Vec<String> {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = compress_card(&mut cx);
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t tab");
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t");
    if by_click {
        assert!(button_is(&mut cx, handle, 1, "archive.compress.action"));
        let compress = drawn(&mut cx, handle, "op2-button-1");
        click(&mut cx, handle, compress.center());
    } else {
        keys(&mut cx, handle, "enter");
    }
    assert!(
        !card_open(&mut cx, handle),
        "the submission closes the card"
    );
    archive_lines(&mut requests)
}

#[test]
fn b02_a_click_on_compress_submits_exactly_as_enter_does() {
    let clicked = submitted(true);
    assert!(
        !clicked.is_empty(),
        "the click asks the backend to compress"
    );
    assert_eq!(clicked, submitted(false));
}

/// The archive lines one Extract retry sends, pressed by a click on the button or by Enter on it.
fn extract_submitted(by_click: bool) -> Vec<String> {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = extract_card(&mut cx);
    keys(&mut cx, handle, "s->s e->e c->c");
    if by_click {
        assert!(button_is(&mut cx, handle, 1, "archive.extract.action"));
        let extract = drawn(&mut cx, handle, "op2-button-1");
        click(&mut cx, handle, extract.center());
    } else {
        keys(&mut cx, handle, "enter");
    }
    archive_lines(&mut requests)
}

#[test]
fn b02_a_click_on_extract_submits_exactly_as_enter_does() {
    let clicked = extract_submitted(true);
    assert!(!clicked.is_empty(), "the click asks the backend to extract");
    assert_eq!(clicked, extract_submitted(false));
}
