//! The words a send shows, in English and through a language change.
use super::*;

#[test]
fn sd2_board_words_in_english() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let mut send = sending();
    assert_eq!(present::title(&i18n, 3, "gdrive"), "Send 3 Items to gdrive");
    assert_eq!(present::title(&i18n, 1, "gdrive"), "Send 1 Item to gdrive");
    assert_eq!(
        (
            present::row_label(&i18n).as_str(),
            present::folder_label(&i18n).as_str()
        ),
        ("Send with rclone", "Folder")
    );
    assert_eq!(
        (
            present::send_label(&i18n).as_str(),
            present::cancel_label(&i18n).as_str()
        ),
        ("Send", "Cancel")
    );
    assert_eq!(
        present::card_error(&i18n, Some(FolderRefusal::Empty), None),
        None
    );
    let block = present::block(&i18n, &send).unwrap();
    assert_eq!(block.head, "Sending 2 of 3 to gdrive");
    assert_eq!(block.file, "DSC_4821.jpg · 565.8 kB");
    assert_eq!(
        text(&block.bytes),
        "795.1 kB of 1.7 MB · 240.0 kB/s · 0:03 left"
    );
    assert_eq!(
        block
            .bytes
            .iter()
            .filter(|part| part.figure)
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>(),
        ["795.1 kB", "1.7 MB", "240.0 kB/s", "0:03"]
    );
    assert_eq!(block.cancel, Some(("Cancel".into(), true)));
    assert!(matches!(block.bar, Bar::Fill(share) if (share - 795120. / 1702100.).abs() < 1e-6));
    assert!(send.cancel().is_some());
    assert_eq!(
        present::block(&i18n, &send).unwrap().cancel,
        Some(("Cancelling".into(), false))
    );
    send.cancelling = false;
    assert!(send.apply(&job(r#""state":"running","phase":"checking","check":"checksum","file":"comparing checksums","done":3,"total":3"#)));
    let block = present::block(&i18n, &send).unwrap();
    assert_eq!(
        (block.head.as_str(), block.file.as_str(), block.bar),
        (
            "Checking 3 items on gdrive",
            "comparing checksums",
            Bar::Sweep
        )
    );
    assert!(block.bytes.is_empty() && block.cancel.is_some());
    assert!(send.apply(&job(
        r#""state":"done","phase":"checked","check":"checksum","done":3,"total":3"#
    )));
    assert!(send.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":72,"ok":true,"cancelled":false,"check":"checksum","receipt":"Sent 3 items to Uploads on gdrive","note":"","error":""}"#)));
    assert!(
        present::block(&i18n, &send).is_none(),
        "a checked send leaves the card"
    );
    let receipt = present::receipt(&i18n, &send).unwrap();
    assert_eq!(
        (receipt.text.as_str(), receipt.hint.as_str()),
        (
            "Sent 3 items to Uploads on gdrive",
            " · checked by checksum"
        )
    );
}

#[test]
fn sd2_failure_size_not_checked_and_root_words() {
    let root = Root::new();
    let i18n = root.i18n("en");
    let mut send = sending();
    assert!(send.apply(&job(
        r#""state":"failed","reason":"DSC_4930.jpg did not match on b2-photos.""#
    )));
    let block = present::block(&i18n, &send).unwrap();
    assert_eq!(
        (block.head.as_str(), block.file.as_str()),
        ("Send failed", "DSC_4930.jpg did not match on b2-photos.")
    );
    assert_eq!(
        (
            block.retry.as_deref(),
            block.dismiss.as_deref(),
            block.bar,
            block.failed
        ),
        (Some("Retry"), Some("Dismiss"), Bar::None, true)
    );
    assert!(block.cancel.is_none() && present::receipt(&i18n, &send).is_none());
    let mut sized = sending();
    assert!(sized.apply(&job(
        r#""state":"running","phase":"checking","check":"size","total":3"#
    )));
    assert_eq!(
        present::block(&i18n, &sized).unwrap().file,
        "comparing sizes"
    );
    assert!(sized.apply(&job(
        r#""state":"done","phase":"checked","check":"size","done":3,"total":3"#
    )));
    assert!(sized.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":72,"ok":true,"cancelled":false,"check":"size","receipt":"r","note":"","error":""}"#)));
    assert_eq!(
        present::receipt(&i18n, &sized).unwrap().hint,
        " · checked by size"
    );
    let mut stopped = Send::new(72, "gdrive", "", 3);
    assert!(stopped.apply(&event(
        r#"{"t":"rclonestarted","requestId":72,"id":12,"ok":true,"error":""}"#
    )));
    assert!(stopped.apply(&job(
        r#""state":"running","phase":"checking","check":"checksum","done":3,"total":3"#
    )));
    assert!(stopped.apply(&job(
        r#""state":"done","phase":"finished","check":"none","done":3,"total":3,"cancelled":true"#
    )));
    assert!(stopped.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":72,"ok":false,"cancelled":true,"check":"none","receipt":"Sent 3 items to gdrive","note":"not checked","error":""}"#)));
    let receipt = present::receipt(&i18n, &stopped).unwrap();
    assert_eq!(
        (receipt.text.as_str(), receipt.hint.as_str()),
        ("Sent 3 items to gdrive", " · not checked")
    );
    let mut refused = Send::new(81, "gdrive", "x", 1);
    assert!(refused.apply(&event(r#"{"t":"rclonestarted","requestId":81,"ok":false,"error":"rclone remote gdrive is no longer configured."}"#)));
    assert_eq!(
        present::card_error(&i18n, None, Some(&refused)).as_deref(),
        Some("rclone remote gdrive is no longer configured.")
    );
    assert_eq!(
        present::card_error(&i18n, Some(FolderRefusal::Invalid), None).as_deref(),
        Some("An rclone folder cannot contain a .. component or a control character.")
    );
    let mut lost = sending();
    assert!(lost.backend_ended());
    let block = present::block(&i18n, &lost).unwrap();
    assert_eq!(
        (
            block.head.as_str(),
            block.file.as_str(),
            block.retry.is_none()
        ),
        ("Send failed", "", true)
    );
}

#[test]
fn sd2_locale_witness_translates_falls_back_and_follows_a_language_change() {
    let root = Root::new();
    root.write(
        "catalog/de/flea.json",
        r#"{"send.rclone.failed":"Senden fehlgeschlagen","send.rclone.checking":{"one":"Prüfe %1 Datei auf %2","other":"Prüfe %1 Dateien auf %2"}}"#,
    );
    let mut i18n = root.i18n("en");
    let mut send = sending();
    assert!(send.apply(&job(
        r#""state":"running","phase":"checking","check":"checksum","total":3"#
    )));
    assert_eq!(
        present::block(&i18n, &send).unwrap().head,
        "Checking 3 items on gdrive"
    );
    root.write("language", "de\n");
    assert!(i18n.poll_changed());
    let block = present::block(&i18n, &send).unwrap();
    assert_eq!(block.head, "Prüfe 3 Dateien auf gdrive");
    assert_eq!(
        block.file, "comparing checksums",
        "an untranslated string falls back to English"
    );
    assert_eq!(
        block.cancel.unwrap().0,
        "Abbrechen",
        "the Omarchy reference follows the language"
    );
    assert!(send.apply(&job(
        r#""state":"failed","reason":"Could not send a.jpg to gdrive. 401 Unauthorized""#
    )));
    let block = present::block(&i18n, &send).unwrap();
    assert_eq!(block.head, "Senden fehlgeschlagen");
    assert_eq!(
        block.file, "Could not send a.jpg to gdrive. 401 Unauthorized",
        "the backend's sentence stays an opaque diagnostic"
    );
    assert!(
        !block.head.contains("send.rclone") && !block.file.contains("send.rclone"),
        "no key leaks"
    );
}

#[test]
fn sd2_the_check_sweep_stays_on_its_track() {
    let mut last = -1.;
    for at in (0..SWEEP_PERIOD_MS).step_by(10) {
        let (left, width) = sweep(at);
        assert!(
            (0. ..=1.).contains(&left)
                && width >= 0.
                && left + width <= 1. + 1e-6
                && width <= SWEEP_SHARE + 1e-6,
            "{at}"
        );
        assert!(left >= last, "one pass moves one way");
        last = left;
    }
    assert_eq!(sweep(0), sweep(SWEEP_PERIOD_MS), "the pass repeats");
    assert_eq!(
        present::duration(&crate::i18n::Locale::default(), 3725),
        "1:02:05"
    );
}
