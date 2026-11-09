//! Extract through the production window: the request by archive, the password card on its own answer, retry and refusal.
use super::*;

#[test]
fn m9_integ_extract_answers_only_the_asked_archive() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);

    edit(&mut cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![2];
    });
    // Extract goes beside the archive; only the asked archive's encrypted answer opens the password card.
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.7z"]);
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"archive","op":"extract","path":"/files/photos.7z","dest":"/files/photos","menuId":43}"#,
            r#"{"c":"menuaction","op":"close","id":43}"#,
        ]
    );
    receive(&mut cx, handle, 0, r#"{"t":"archivestarted","id":7}"#);
    receive(&mut cx, handle, 0, r#"{"t":"extractstarted","id":7}"#);
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":6,"ok":false,"verified":true,"err":"encrypted","path":"/files/stale.zip","dest":"/files/stale"}"#,
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
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.7z","dest":"/files/photos"}"#,
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
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.7z","dest":"/files/photos"}"#,
    );
    assert!(
        edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .extract
            .opened()),
        "the asked archive opens the card"
    );
    assert!(
        VisualTestContext::from_window(handle.into(), &cx)
            .debug_bounds("op2-card")
            .is_some(),
        "the password-needed result draws the password card"
    );
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t");
    keys(&mut cx, handle, "enter");
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"archive","op":"extract","path":"/files/photos.7z","dest":"/files/photos","password":"secret","menuId":0}"#
        ]
    );
    // A wrong password reopens the card holding the bullets of the refused answer, never its characters.
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":8,"ok":false,"verified":true,"err":"wrong-password","path":"/files/photos.7z","dest":"/files/photos"}"#,
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

// An encrypted reply on the other side must leave the active password card with its original backend.
#[test]
fn an_extract_card_keeps_its_backend_when_the_other_side_needs_a_password() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, mut other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    edit(&mut cx, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .extracts
            .push((1, "/right/b.zip".into(), "/right/b".into()))
    });
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    receive(
        &mut cx,
        handle,
        1,
        r#"{"t":"archivedone","id":8,"ok":false,"verified":true,"err":"encrypted","path":"/right/b.zip","dest":"/right/b"}"#,
    );
    let side = edit(&mut cx, handle, |shell, _, _| {
        shell.operations.archive.card_side
    });
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t");
    keys(&mut cx, handle, "enter");
    let (left, right) = (drain(&mut requests), drain(&mut other));
    eprintln!("card_side={side} left={left:?} right={right:?}");
    assert_eq!(side, 0, "the open card still belongs to side 0");
    assert_eq!(
        left.iter()
            .filter(|line| line.contains("photos.zip") && line.contains(r#""password":"secret""#))
            .count(),
        1,
        "the owning backend receives the password once: {left:?}"
    );
    assert!(
        right.iter().all(|line| !line.contains("photos.zip")),
        "side 0's password never goes to side 1: {right:?}"
    );
}

// The shipped catalogs with German chosen, from a marked mktemp root that holds only the roster and the language file.
fn shipped_german() -> flea::i18n::I18n {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let parent = repo.join(".superpowers/fx-case-gui");
    std::fs::create_dir_all(&parent).unwrap();
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("de.XXXXXXXX"))
        .output()
        .unwrap();
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(made.status.success() && root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "fx-case gui\n").unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        include_str!("../../../../../../src/i18n/fixtures/locales.json"),
    )
    .unwrap();
    std::fs::write(root.join("language"), "de\n").unwrap();
    flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![repo.join("i18n")],
    })
}

#[test]
fn an_extract_refusal_reads_in_the_window_language() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests, _other) = start(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        shell.locale.catalog = std::sync::Arc::new(shipped_german());
    });
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":9,"ok":false,"verified":true,"err":"'a.txt' and 'A.txt' would take one name on this drive, so nothing was extracted"}"#,
    );
    let status = footer(&mut cx, handle);
    assert!(
        status.contains("a.txt") && status.contains("denselben Namen, daher wurde nichts entpackt"),
        "the refusal is said in German: {status}"
    );
}
