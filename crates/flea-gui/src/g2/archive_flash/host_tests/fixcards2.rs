//! EY-542, EY-326, EY-547: an extract's job row ends with its archive line, so no Extracting card outlives it.
use super::*;

// Sample: the extract job row the backend's Jobs::update writes, here running, failed on a password, or done.
fn job(id: u64, state: &str, reason: &str, total: u64) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"extract","head":"Extracting","file":"two.txt","done":{total},"total":{total},"bytes":63,"totalBytes":63,"itemBytes":14,"rate":0,"left":null,"state":"{state}","reason":"{reason}","output":[],"undo":false,"cancelled":false,"phase":"","check":"","receipt":"","note":""}},"unfinished":{}}}"#,
        u8::from(state == "running")
    )
}

/// The lines start_archive and the job worker write for one extract, in the backend's order, up to its job's last row.
fn run_extract(
    cx: &mut TestAppContext,
    handle: Handle,
    id: u64,
    leaf: &str,
    end: &str,
    reason: &str,
) {
    receive(
        cx,
        handle,
        0,
        &format!(r#"{{"t":"archivestarted","id":{id}}}"#),
    );
    receive(
        cx,
        handle,
        0,
        &format!(r#"{{"t":"extractstarted","id":{id}}}"#),
    );
    receive(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"transferprogress","id":{id},"index":0,"name":"{leaf}","bytes":0,"total":0,"scanned":0}}"#
        ),
    );
    receive(cx, handle, 0, &job(id, "running", "", 0));
    receive(cx, handle, 0, &job(id, end, reason, 3));
}

fn extracting_card(cx: &mut TestAppContext, handle: Handle) -> bool {
    edit(cx, handle, |shell, _, cx| shell.jobs_card_showing(cx))
}

#[test]
fn fixcards2_ey542_a_finished_extract_leaves_no_extracting_card() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    run_extract(&mut cx, handle, 7, "photos.zip", "done", "");
    assert!(
        extracting_card(&mut cx, handle),
        "the card shows while the extract's archive line is still out"
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":true,"verified":true,"err":"","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert!(
        !extracting_card(&mut cx, handle),
        "a finished extract leaves no Extracting card with a live Cancel"
    );
    let status = call(&mut cx, handle, "statusActivityState");
    let card = flea::jsondoc::parse(status.trim()).unwrap();
    assert_eq!(
        card.get("transferCard")
            .and_then(|card| card.get("visible"))
            .and_then(Json::as_bool),
        Some(false),
        "{status}"
    );
}

#[test]
fn fixcards2_ey326_the_password_card_opens_alone() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    run_extract(&mut cx, handle, 7, "photos.zip", "failed", "encrypted");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert!(edit(&mut cx, handle, |shell, _, _| shell
        .operations
        .archive
        .extract
        .opened()));
    assert!(
        !extracting_card(&mut cx, handle),
        "no Extracting card beside the password card"
    );
    // A wrong password ends its own extract the same way.
    keys(&mut cx, handle, "w->w r->r o->o n->n g->g enter");
    run_extract(&mut cx, handle, 8, "photos.zip", "failed", "wrong-password");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":8,"ok":false,"verified":true,"err":"wrong-password","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert!(
        !extracting_card(&mut cx, handle),
        "no Extracting card after a wrong password"
    );
}

#[test]
fn fixcards2_ey547_escape_closes_the_password_card_after_a_wrong_password() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    run_extract(&mut cx, handle, 7, "photos.zip", "failed", "encrypted");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    keys(&mut cx, handle, "w->w r->r o->o n->n g->g enter");
    run_extract(&mut cx, handle, 8, "photos.zip", "failed", "wrong-password");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":8,"ok":false,"verified":true,"err":"wrong-password","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    let open = call(&mut cx, handle, "archiveCardsState");
    assert!(open.contains(r#""extractOpened":true"#), "{open}");
    keys(&mut cx, handle, "escape");
    let closed = call(&mut cx, handle, "archiveCardsState");
    assert!(
        !closed.contains(r#""extractOpened":true"#),
        "Escape closes the card: {closed}"
    );
    assert!(edit(&mut cx, handle, |shell, _, _| !shell
        .operations
        .archive
        .showing()));
}

#[test]
fn fixcards2_ey542_a_failed_extract_still_reads_as_failed() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    run_extract(
        &mut cx,
        handle,
        7,
        "photos.zip",
        "failed",
        "photos.zip is corrupt",
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"photos.zip is corrupt"}"#,
    );
    let head = edit(&mut cx, handle, |shell, _, cx| {
        shell.overlay.as_ref().and_then(|overlay| {
            let model = &overlay.read(cx).model;
            model
                .jobs
                .blocks()
                .next()
                .map(flea::ui_logic_file_ops::jobs::state)
        })
    });
    assert_eq!(
        head,
        Some(flea::ui_logic_file_ops::jobs::State::Failed),
        "a real failure keeps its failed block with Retry and Dismiss"
    );
}

fn assert_no_extract_job(cx: &mut TestAppContext, handle: Handle) {
    assert!(!extracting_card(cx, handle), "no stale Extracting card");
    edit(cx, handle, |shell, _, cx| {
        let model = &shell.overlay.as_ref().unwrap().read(cx).model;
        assert!(!model.wire.transfer.running);
        assert_eq!(model.jobs.blocks().count(), 0);
        assert_eq!(model.jobs.unfinished(), 0);
    });
    let status = call(cx, handle, "statusActivityState");
    let state = flea::jsondoc::parse(status.trim()).unwrap();
    assert_eq!(
        state
            .get("transferCard")
            .and_then(|card| card.get("visible"))
            .and_then(Json::as_bool),
        Some(false),
        "{status}"
    );
}

#[test]
fn sol_extract_pw_card_cancel_and_wrong_password_leave_no_terminal_job() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    run_extract(&mut cx, handle, 7, "photos.zip", "failed", "encrypted");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert_no_extract_job(&mut cx, handle);
    keys(&mut cx, handle, "w->w r->r o->o n->n g->g enter");
    assert_eq!(drain(&mut requests).len(), 1, "one password retry");
    run_extract(&mut cx, handle, 8, "photos.zip", "failed", "wrong-password");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":8,"ok":false,"verified":true,"err":"wrong-password","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert_no_extract_job(&mut cx, handle);
    assert!(call(&mut cx, handle, "archiveCardsState").contains(r#""extractOpened":true"#));
    // Use the actual Cancel button's keyboard action, not Escape or direct model cleanup.
    keys(&mut cx, handle, "tab enter");
    assert!(!call(&mut cx, handle, "archiveCardsState").contains(r#""extractOpened":true"#));
    assert_no_extract_job(&mut cx, handle);
    assert!(
        drain(&mut requests).is_empty(),
        "Cancel starts no extraction"
    );
}

#[test]
fn sol_extract_pw_card_correct_password_has_one_live_job_then_none() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(&mut cx, handle, 43, "extract", "", &["/files/photos.zip"]);
    drain(&mut requests);
    run_extract(&mut cx, handle, 7, "photos.zip", "failed", "encrypted");
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":7,"ok":false,"verified":true,"err":"encrypted","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert_no_extract_job(&mut cx, handle);
    keys(&mut cx, handle, "s->s e->e c->c r->r e->e t->t enter");
    assert_eq!(
        drain(&mut requests),
        [
            r#"{"c":"archive","op":"extract","path":"/files/photos.zip","dest":"/files/photos","password":"secret","menuId":0}"#
        ]
    );
    run_extract(&mut cx, handle, 8, "photos.zip", "running", "");
    assert!(!call(&mut cx, handle, "archiveCardsState").contains(r#""extractOpened":true"#));
    assert!(
        extracting_card(&mut cx, handle),
        "real extraction keeps progress"
    );
    edit(&mut cx, handle, |shell, _, cx| {
        let model = &shell.overlay.as_ref().unwrap().read(cx).model;
        assert!(model.wire.transfer.running);
        let ids: Vec<_> = model.jobs.blocks().map(|job| job.id).collect();
        assert_eq!(ids, [8], "only the retry is a live progress card");
    });
    receive(&mut cx, handle, 0, &job(8, "done", "", 3));
    assert!(
        extracting_card(&mut cx, handle),
        "wait for the real archive end"
    );
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":8,"ok":true,"verified":true,"err":"","path":"/files/photos.zip","dest":"/files/photos"}"#,
    );
    assert_no_extract_job(&mut cx, handle);
    // Soak S1B-6: an extract's receipt is its own, never the compress receipt.
    let said = footer(&mut cx, handle);
    assert!(said.contains("Extracted."), "{said}");
    assert!(!said.contains("could not be read"), "{said}");
    assert!(!said.contains("Archive written."), "{said}");
}

#[test]
fn fixcards2_s1b6_a_finished_compress_keeps_the_archive_receipt() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut requests, _other) = start(&mut cx);
    choose(
        &mut cx,
        handle,
        44,
        "compress",
        "zip",
        &["/files/notes.txt"],
    );
    assert!(drain(&mut requests)
        .iter()
        .any(|line| line.contains(r#""op":"compress""#)));
    receive(&mut cx, handle, 0, r#"{"t":"archivestarted","id":9}"#);
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"archivedone","id":9,"ok":true,"verified":true,"err":""}"#,
    );
    let said = footer(&mut cx, handle);
    assert!(said.contains("Archive written."), "{said}");
}
