//! Extract through the production window: the request by archive, the password card on its own answer, retry and refusal.
use super::*;

#[test]
fn m9_integ_extract_answers_only_the_asked_archive() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);

    // Extract goes beside the archive; only the asked archive's encrypted answer opens the password card.
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"archive","op":"extract","path":"/files/photos.zip","dest":"/files/photos","menuId":43}"#,
            r#"{"c":"menuaction","op":"close","id":43}"#,
        ]
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/stale.zip","dest":"/files/stale"}"#,
    );
    assert!(
        edit(&mut cx, handle, |shell, _, _| !shell
            .operations
            .archive
            .extract
            .opened()),
        "a stale reply opens nothing"
    );
    receive(
        &mut cx,
        handle,
        1,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert!(
        edit(&mut cx, handle, |shell, _, _| !shell
            .operations
            .archive
            .extract
            .opened()),
        "another backend's reply opens nothing"
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .extract
            .opened()),
        "the asked archive opens the card"
    );
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t");
    keys(&mut cx, handle, "enter");
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"archive","op":"extract","path":"/files/photos.zip","dest":"/files/photos","password":"secret","menuId":0}"#
        ]
    );
    // A wrong password reopens the card holding the bullets of the refused answer, never its characters.
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":8,"ok":false,"verified":true,"err":"wrong-password","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    edit(&mut cx, handle, |shell, _, _| {
        assert!(shell.operations.archive.extract.opened());
        assert_eq!(shell.operations.archive.extract.held_marks(), 6);
    });
    keys(&mut cx, handle, "escape");
    assert!(
        edit(&mut cx, handle, |shell, _, _| !shell
            .operations
            .archive
            .showing()),
        "Escape closes the card"
    );
    // A free-space refusal is said in the backend's own words, as an error.
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":9,"ok":false,"verified":true,"err":"not enough free space"}"#,
    );
    let status = footer(&mut cx, handle);
    assert!(
        status.contains("Not enough free space"),
        "the refusal is said: {status}"
    );
}
