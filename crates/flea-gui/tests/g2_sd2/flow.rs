//! The menu, the card and each send from Send to its end, sharing the fake backend in `g2_sd2.rs`.
use super::*;

#[test]
fn menu_row_waits_for_its_own_remotes_reply_and_hides_when_rclone_is_absent() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    assert_eq!(send.menu_row(&i18n, &[]), None);
    send.menu_opened(&fake);
    let first = id_of(&fake.last());
    assert_eq!(
        fake.last(),
        format!(r#"{{"c":"rclone","op":"remotes","id":{first}}}"#)
    );
    send.menu_opened(&fake);
    let second = id_of(&fake.last());
    assert!(
        !send.on_line(&format!(
            r#"{{"t":"rcloneremotes","id":{first},"visible":true,"remotes":["stale"]}}"#
        )),
        "an older menu's answer is stale"
    );
    assert_eq!(send.menu_row(&i18n, &[]), None);
    assert!(send.on_line(&format!(r#"{{"t":"rcloneremotes","id":{second},"visible":true,"remotes":["gdrive","b2-photos","nas-crypt"]}}"#)));
    let row = send.menu_row(&i18n, &[]).unwrap();
    assert_eq!(
        (row.id, row.label.as_str(), row.mark, row.chevron, row.lit),
        (
            "sendRclone",
            "Send with rclone",
            "cloud-upload",
            true,
            false
        )
    );
    send.open_flyout(true);
    assert!(send.menu_row(&i18n, &[]).unwrap().lit);
    let flyout: Vec<_> = send
        .flyout()
        .into_iter()
        .map(|row| (row.label, row.mark))
        .collect();
    assert_eq!(
        flyout,
        [
            ("gdrive".into(), "server"),
            ("b2-photos".into(), "server"),
            ("nas-crypt".into(), "server")
        ]
    );
    assert_eq!(
        send.menu_row(&i18n, &["sendRclone".into()]),
        None,
        "Settings > Menus hides it"
    );
    send.menu_opened(&fake);
    let third = id_of(&fake.last());
    assert!(send.on_line(&format!(
        r#"{{"t":"rcloneremotes","id":{third},"visible":false,"remotes":[]}}"#
    )));
    assert_eq!(send.menu_row(&i18n, &[]), None);
    send.menu_opened(&fake);
    let fourth = id_of(&fake.last());
    assert!(send.on_line(&format!(
        r#"{{"t":"rcloneremotes","id":{fourth},"visible":true,"remotes":[]}}"#
    )));
    assert_eq!(send.menu_row(&i18n, &[]), None, "no remote, no row");
    for bad in [
        format!(r#"{{"t":"rcloneremotes","id":{fourth},"visible":true,"remotes":"gdrive"}}"#),
        format!(r#"{{"t":"rcloneremotes","id":{fourth},"visible":true,"remotes":[7]}}"#),
        r#"{"t":"rcloneremotes","id":"#.into(),
    ] {
        assert!(!send.on_line(&bad), "{bad}");
    }
}

#[test]
fn card_keeps_the_board_words_remembers_the_folder_and_refuses_bad_folders() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    assert!(!send.choose("unknown", paths()));
    assert!(!send.choose("gdrive", Vec::new()));
    assert!(send.choose("gdrive", paths()));
    let card = send.card(&i18n).unwrap();
    assert_eq!(
        (
            card.title.as_str(),
            card.field_label.as_str(),
            card.value.as_str(),
            card.cancel.as_str(),
            card.primary.as_str()
        ),
        ("Send 3 Items to gdrive", "Folder", "", "Cancel", "Send")
    );
    assert!(
        !card.can_submit && !card.selected_all && card.error.is_none(),
        "a never-used remote opens empty with Send dimmed"
    );
    assert!(!send.submit(&fake));
    send.edit("a/../b");
    let card = send.card(&i18n).unwrap();
    assert_eq!(
        card.error.as_deref(),
        Some("An rclone folder cannot contain a .. component or a control character.")
    );
    assert!(!card.can_submit && !send.submit(&fake));
    send.edit("Uploads");
    let before = fake.count();
    assert!(send.submit(&fake));
    assert_eq!(fake.count(), before + 1);
    let request = id_of(&fake.last());
    assert_eq!(
        fake.last(),
        format!(
            r#"{{"c":"rclone","op":"send","id":{request},"remote":"gdrive","folder":"Uploads","paths":["{}","{}","{}"]}}"#,
            PATHS[0], PATHS[1], PATHS[2]
        )
    );
    assert!(
        !send.card(&i18n).unwrap().can_submit,
        "busy until the backend answers"
    );
    assert!(!send.submit(&fake), "a second press sends nothing");
    assert!(send.on_line(&format!(r#"{{"t":"rclonestarted","requestId":{request},"ok":false,"error":"No remote named gdrive."}}"#)));
    let card = send.card(&i18n).unwrap();
    assert_eq!(card.error.as_deref(), Some("No remote named gdrive."));
    assert!(card.can_submit);
    send.close_card();
    assert!(
        send.blocks(&i18n).is_empty(),
        "a refused send leaves no block"
    );
    assert!(send.choose("gdrive", paths()));
    let card = send.card(&i18n).unwrap();
    assert_eq!(
        (card.value.as_str(), card.selected_all, card.can_submit),
        ("Uploads", true, true)
    );
    assert!(send.choose("b2-photos", paths()));
    assert_eq!(
        send.card(&i18n).unwrap().value,
        "",
        "folders are remembered per remote"
    );
}

#[test]
fn sending_checking_and_sent_follow_the_jobs_in_order_and_ignore_stale_lines() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    let request = started(&mut send, &fake);
    assert!(
        send.card(&i18n).is_none(),
        "the card closes once the backend takes the send"
    );
    assert!(
        !send.on_line(&format!(
            r#"{{"t":"rclonestarted","requestId":{request},"id":99,"ok":true,"error":""}}"#
        )),
        "a repeated start is stale"
    );
    assert!(send.on_line(&job(12, r#""state":"running","phase":"sending","check":"none","file":"DSC_4821.jpg","done":1,"total":3,"bytes":795100,"totalBytes":1702200,"itemBytes":565800,"rate":240000,"left":3,"reason":"","cancelled":false"#)));
    let (id, block) = send.blocks(&i18n)[0].clone();
    assert_eq!(
        (id, block.head.as_str(), block.file.as_str()),
        (
            request,
            "Sending 2 of 3 to gdrive",
            "DSC_4821.jpg · 565.8 kB"
        )
    );
    let bytes: String = block.bytes.iter().map(|part| part.text.as_str()).collect();
    assert_eq!(bytes, "795.1 kB of 1.7 MB · 240.0 kB/s · 0:03 left");
    assert_eq!(block.cancel, Some(("Cancel".into(), true)));
    assert!(
        matches!(block.bar, Bar::Fill(share) if (share - 0.4671).abs() < 5e-5),
        "rclone-sending.html fills width:46.71%"
    );
    assert!(
        !send.on_line(&job(
            13,
            r#""state":"running","phase":"sending","done":2,"total":3"#
        )),
        "another job's row"
    );
    assert!(send.sweep(request).is_none());
    assert!(send.on_line(&job(12, r#""state":"running","phase":"checking","check":"checksum","done":3,"total":3,"bytes":1700000,"totalBytes":1700000"#)));
    let block = send.blocks(&i18n)[0].clone().1;
    assert_eq!(
        (block.head.as_str(), block.file.as_str(), block.bar),
        (
            "Checking 3 items on gdrive",
            "comparing checksums",
            Bar::Sweep
        )
    );
    let sweep = send.sweep(request).unwrap();
    assert!(
        !send.on_line(&job(
            12,
            r#""state":"running","phase":"sending","done":2,"total":3"#
        )),
        "a checking job never steps back"
    );
    assert!(
        Rc::ptr_eq(&sweep, &send.sweep(request).unwrap()),
        "one sweep per checking job"
    );
    assert!(send.on_line(&job(
        12,
        r#""state":"done","phase":"checked","check":"checksum","done":3,"total":3,"receipt":"Sent 3 items to Uploads on gdrive","note":"checked by checksum""#
    )));
    assert!(send.receipt(&i18n).is_none(), "done waits for rclonesent");
    assert!(send.on_line(&format!(r#"{{"t":"rclonesent","id":12,"requestId":{request},"ok":true,"cancelled":false,"check":"checksum","receipt":"Sent 3 items to Uploads on gdrive","note":"checked by checksum","error":""}}"#)));
    assert!(send.blocks(&i18n).is_empty() && send.sweep(request).is_none());
    let receipt = send.receipt(&i18n).unwrap();
    assert_eq!(
        (receipt.text.as_str(), receipt.hint.as_str()),
        (
            "Sent 3 items to Uploads on gdrive",
            " · checked by checksum"
        )
    );
    assert!(
        !send.on_line(&format!(
            r#"{{"t":"rclonesent","id":12,"requestId":{request},"ok":false,"cancelled":false,"check":"none","receipt":"","note":"","error":"late"}}"#
        )),
        "nothing after the end"
    );
    send.receipt_shown();
    assert!(send.receipt(&i18n).is_none());
}

#[test]
fn cancel_sends_once_and_a_cancelled_check_leaves_a_not_checked_receipt() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    let request = started(&mut send, &fake);
    assert!(send.on_line(&job(
        12,
        r#""state":"running","phase":"checking","check":"size","done":3,"total":3"#
    )));
    assert_eq!(send.blocks(&i18n)[0].1.file, "comparing sizes");
    assert!(send.cancel(request, &fake));
    assert_eq!(fake.last(), r#"{"c":"jobs","op":"cancel","id":12}"#);
    assert_eq!(
        send.blocks(&i18n)[0].1.cancel,
        Some(("Cancelling".into(), false))
    );
    assert!(!send.cancel(request, &fake), "a second press sends nothing");
    assert!(send.on_line(r#"{"t":"joberror","id":12,"error":"That job already finished."}"#));
    assert!(
        send.cancel(request, &fake),
        "a refused cancel can be pressed again"
    );
    assert!(send.on_line(&format!(r#"{{"t":"rclonesent","id":12,"requestId":{request},"ok":false,"cancelled":true,"check":"none","receipt":"Sent 3 items to Uploads on gdrive","note":"not checked","error":""}}"#)));
    let receipt = send.receipt(&i18n).unwrap();
    assert_eq!(
        (receipt.text.as_str(), receipt.hint.as_str()),
        ("Sent 3 items to Uploads on gdrive", " · not checked")
    );
    assert!(!send.cancel(request, &fake));
}

#[test]
fn failed_send_offers_retry_and_dismiss_and_a_backend_death_offers_dismiss_only() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    let request = started(&mut send, &fake);
    assert!(send
        .on_line(&job(12, r#""state":"failed","phase":"checking","check":"checksum","done":3,"total":3,"reason":"DSC_4930.jpg did not match on b2-photos.""#)));
    assert!(send.on_line(&format!(r#"{{"t":"rclonesent","id":12,"requestId":{request},"ok":false,"cancelled":false,"check":"checksum","receipt":"","note":"","error":"DSC_4930.jpg did not match on b2-photos."}}"#)));
    let block = send.blocks(&i18n)[0].clone().1;
    assert_eq!(
        (
            block.head.as_str(),
            block.file.as_str(),
            block.failed,
            block.bar
        ),
        (
            "Send failed",
            "DSC_4930.jpg did not match on b2-photos.",
            true,
            Bar::None
        )
    );
    assert_eq!(
        (
            block.retry.as_deref(),
            block.dismiss.as_deref(),
            block.cancel
        ),
        (Some("Retry"), Some("Dismiss"), None)
    );
    assert!(send.retry(request, &fake));
    assert_eq!(fake.last(), r#"{"c":"jobs","op":"retry","id":12}"#);
    assert!(!send.retry(request, &fake), "retry only a failed job");
    assert!(send.on_line(&job(
        12,
        r#""state":"running","phase":"sending","done":0,"total":3"#
    )));
    assert_eq!(send.blocks(&i18n)[0].1.head, "Sending 1 of 3 to gdrive");
    assert!(send.on_line(&job(12, r#""state":"failed","phase":"sending","done":1,"total":3,"reason":"Could not send DSC_4822.jpg to gdrive.""#)));
    assert!(send.dismiss(request, &fake));
    assert_eq!(fake.last(), r#"{"c":"jobs","op":"dismiss","id":12}"#);
    assert!(send.blocks(&i18n).is_empty());

    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    let request = started(&mut send, &fake);
    assert!(send.on_line(&job(
        12,
        r#""state":"running","phase":"sending","done":1,"total":3"#
    )));
    fake.dead.set(true);
    assert!(send.backend_ended());
    let block = send.blocks(&i18n)[0].clone().1;
    assert_eq!(
        (
            block.head.as_str(),
            block.retry,
            block.dismiss.as_deref(),
            block.cancel
        ),
        ("Send failed", None, Some("Dismiss"), None)
    );
    assert_eq!(
        send.menu_row(&i18n, &[]),
        None,
        "the next backend is asked again"
    );
    assert!(!send.cancel(request, &fake) && !send.retry(request, &fake));
    let before = fake.lines.borrow().len();
    assert!(
        send.dismiss(request, &fake),
        "a lost send leaves without a request"
    );
    assert_eq!(fake.lines.borrow().len(), before);
    assert!(send.blocks(&i18n).is_empty());
    send.menu_opened(&fake);
    assert!(
        !send.on_line(r#"{"t":"rcloneremotes","id":1,"visible":true,"remotes":["gdrive"]}"#),
        "no request went out, so no reply is current"
    );
}
