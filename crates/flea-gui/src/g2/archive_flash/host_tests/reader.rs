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
        extract.focus(crate::g2::archive_flash::extract::Stop::Cancel);
    });
    frame(&mut cx, handle);
    let state = call(&mut cx, handle, "archiveCardsState");
    assert!(state.contains("\"extractFocus\":1"), "{state}");
    assert!(
        eye_x(&state) > card_middle(&mut cx, handle),
        "the empty field's eye sits at the right edge, not after its text: {state}"
    );
}

// The eye one capture answers, never the other card's.
fn capture_eye(state: &str, capture: &str) -> String {
    let doc = flea::jsondoc::parse(state.trim()).unwrap();
    doc.get(capture)
        .and_then(|capture| capture.get("eye"))
        .and_then(Json::as_str)
        .expect("the capture names its eye")
        .to_owned()
}

// archiveCardsState inside the caller's own update, before the frame that update's end draws.
fn read_now(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) -> String {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: "archiveCardsState".into(),
        arguments: vec![],
    };
    shell.ipc(Event::Call { request, answer }, window, cx);
    receive.try_recv().unwrap().reply.stdout
}

#[test]
fn ey325_a_card_reads_no_eye_before_it_paints() {
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
    let compress = call(&mut cx, handle, "archiveCardsState");
    assert!(
        eye_x(&compress) > card_middle(&mut cx, handle),
        "{compress}"
    );
    keys(&mut cx, handle, "escape");
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.7z"]);
    receive(&mut cx, handle, 0, r#"{"t":"archivestarted","id":7}"#);
    receive(&mut cx, handle, 0, r#"{"t":"extractstarted","id":7}"#);
    // The encrypted answer opens the extract card and the read lands in the same update, before its first frame.
    let unpainted = edit(&mut cx, handle, |shell, window, cx| {
        let line = r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.7z","dest":"/files/photos"}"#;
        shell.receive(0, Backend::test_reply_line(line), window, cx);
        read_now(shell, window, cx)
    });
    assert!(unpainted.contains("\"extractOpened\":true"), "{unpainted}");
    assert_eq!(
        capture_eye(&unpainted, "extractCapture"),
        "",
        "the compress card's eye never answers for the extract card: {unpainted}"
    );
    frame(&mut cx, handle);
    let painted = call(&mut cx, handle, "archiveCardsState");
    assert!(eye_x(&painted) > card_middle(&mut cx, handle), "{painted}");
    // The other way round: a compress card opened after the painted extract card.
    keys(&mut cx, handle, "escape");
    let unpainted = edit(&mut cx, handle, |shell, window, cx| {
        let compress = chosen(44, "compress", "compressPassword", &["/files/notes.txt"]);
        assert!(shell.archive_chosen(&compress, window, cx));
        read_now(shell, window, cx)
    });
    assert!(unpainted.contains("\"compressOpened\":true"), "{unpainted}");
    assert_eq!(
        capture_eye(&unpainted, "compressCapture"),
        "",
        "the extract card's eye never answers for the compress card: {unpainted}"
    );
}
