//! EY-325: the password cards answer archiveCardsState after real typed keys, and the eye paints at the field's right edge.
use super::*;

fn eye_x(state: &str) -> f32 {
    let doc = flea::jsondoc::parse(state.trim()).unwrap();
    let capture = ["compressCapture", "extractCapture"]
        .into_iter()
        .filter_map(|key| doc.get(key))
        .find_map(|capture| {
            capture
                .get("eye")
                .and_then(Json::as_str)
                .filter(|e| !e.is_empty())
        })
        .expect("a painted eye centre");
    capture.split(' ').next().unwrap().parse().unwrap()
}

fn card_middle(cx: &mut TestAppContext, handle: Handle) -> f32 {
    let card = VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op2-card")
        .expect("the card is drawn");
    f32::from(card.center().x)
}

#[test]
fn ey325_typed_pair_reaches_the_reader_and_never_its_characters() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests, _other) = start(&mut cx);
    choose(
        &mut cx,
        handle,
        42,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    let opened = call(&mut cx, handle, "archiveCardsState");
    assert!(opened.contains("\"compressOpened\":true"), "{opened}");
    assert!(opened.contains("\"canCompress\":false"), "{opened}");
    keys(&mut cx, handle, "s->s 3->3 c->c r->r e->e t->t");
    keys(&mut cx, handle, "tab");
    keys(&mut cx, handle, "s->s 3->3 c->c r->r e->e t->t");
    let state = call(&mut cx, handle, "archiveCardsState");
    assert!(state.contains("\"canCompress\":true"), "{state}");
    assert!(state.contains("\"compressFocus\":0"), "{state}");
    assert!(
        !state.contains("s3cret"),
        "the reader never carries the secret"
    );
    assert!(
        eye_x(&state) > card_middle(&mut cx, handle),
        "the Password eye sits at the right edge: {state}"
    );
}

#[test]
fn ey325_an_unfocused_empty_secret_field_draws_its_eye_at_the_right() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests, _other) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        let catalog = shell.locale.catalog.clone();
        let extract = &mut shell.operations.archive.extract;
        extract.open("/files/locked.7z", "/files/locked", "encrypted", &catalog);
        extract.focus(crate::g2::op2::extract::Stop::Cancel);
    });
    frame(&mut cx, handle);
    let state = call(&mut cx, handle, "archiveCardsState");
    assert!(state.contains("\"extractFocus\":1"), "{state}");
    assert!(
        eye_x(&state) > card_middle(&mut cx, handle),
        "the empty field's eye sits at the right edge, not after its text: {state}"
    );
}
