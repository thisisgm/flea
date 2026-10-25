//! Port of the flashpane.js decisions and the flash.js ending inventory.
use super::flash::{Drive, Image};
use super::flashcard::Transfer;
use super::{english, flashendings, flashpane};

fn stick(dev: &str, seq: u64, serial: &str, label: &str, refused: &str) -> Drive {
    Drive {
        dev: dev.into(),
        seq,
        serial: serial.into(),
        label: label.into(),
        model: format!("{label} Disk"),
        bytes: 124623257600,
        refused: refused.into(),
    }
}

fn poll(chosen: Option<Drive>, held: bool, cursor: usize, drives: Vec<Drive>) -> flashpane::Poll {
    flashpane::Poll { chosen, held, cursor, drives, first: false, pending_disk: None }
}

#[test]
fn choice_holds_by_identity_across_polls() {
    let (a, b, c) = (stick("sda", 2, "AAA", "Alpha", ""), stick("sdb", 3, "BBB", "Bravo", ""), stick("sdc", 4, "CCC", "Charlie", ""));
    let reordered = flashpane::reconcile(&poll(Some(b.clone()), false, 3, vec![a.clone(), b.clone()]), &[a.clone(), c.clone(), b.clone()]);
    assert_eq!(reordered.selected, Some(2));
    assert_eq!(reordered.chosen.as_ref().map(|d| d.dev.as_str()), Some("sdb"));
    let swapped = flashpane::reconcile(
        &poll(Some(b.clone()), false, 3, vec![a.clone(), b.clone()]),
        &[a.clone(), stick("sdb", 9, "ZZZ", "Zulu", "")],
    );
    assert!(swapped.chosen.is_none());
    let gone = flashpane::reconcile(&poll(Some(b.clone()), false, 3, vec![a.clone(), b.clone()]), &[a.clone(), c.clone()]);
    assert!(gone.chosen.is_none());
    let lone = flashpane::reconcile(&poll(Some(b.clone()), false, 3, vec![a.clone(), b.clone()]), std::slice::from_ref(&a));
    assert!(lone.chosen.is_none());
    let later = flashpane::reconcile(&poll(lone.chosen, lone.held, lone.cursor, vec![a.clone()]), std::slice::from_ref(&a));
    assert!(later.chosen.is_none());
}

#[test]
fn first_answer_preselects_and_lands_cursor() {
    let a = stick("sda", 2, "AAA", "Alpha", "");
    let small = stick("sdb", 3, "BBB", "Bravo", "small");
    let first = flashpane::Poll { chosen: None, held: false, cursor: 0, drives: Vec::new(), first: true, pending_disk: None };
    let folded = flashpane::reconcile(&first, &[a.clone(), small]);
    assert_eq!(folded.selected, Some(0));
    assert_eq!((folded.cursor, folded.moved), (1, true));
    let mut view = flashpane::ViewMark::default();
    flashpane::land(&mut view, &flashpane::Fold { chosen: Some(a.clone()), held: true, selected: Some(0), cursor: 1, moved: true });
    assert_eq!((view.pending_drive.as_str(), view.chosen == Some(a), view.held, view.cursor), ("", true, true, 1));
}

#[test]
fn poll_keeps_cursor_on_what_it_named() {
    let (a, b, c) = (stick("sda", 2, "AAA", "Alpha", ""), stick("sdb", 3, "BBB", "Bravo", ""), stick("sdc", 4, "CCC", "Charlie", ""));
    let on_flash = flashpane::reconcile(&poll(Some(a.clone()), true, 3, vec![a.clone(), b.clone()]), &[a.clone(), b.clone()]);
    assert_eq!((on_flash.cursor, on_flash.moved), (3, false));
    let grown = flashpane::reconcile(&poll(Some(a.clone()), true, 3, vec![a.clone(), b.clone()]), &[a.clone(), b.clone(), c.clone()]);
    assert_eq!(grown.cursor, 4);
    let on_drive = flashpane::reconcile(&poll(Some(a.clone()), true, 2, vec![a.clone(), b.clone()]), &[c.clone(), a.clone(), b.clone()]);
    assert_eq!(on_drive.cursor, 3);
}

#[test]
fn rail_disk_on_a_partitionless_card_chooses_the_card() {
    let a = stick("sda", 2, "AAA", "Alpha", "");
    let card = stick("mmcblk0", 5, "SD1", "Card", "");
    let first = flashpane::Poll {
        chosen: None,
        held: false,
        cursor: 0,
        drives: Vec::new(),
        first: true,
        pending_disk: Some("/dev/mmcblk0".into()),
    };
    assert_eq!(flashpane::reconcile(&first, &[a, card]).selected, Some(1));
}

#[test]
fn host_mark_follows_the_covered_pane() {
    let mut host = flashpane::Host::default();
    let mut panes = [false, false];
    flashpane::cover(&mut host, &mut panes, 0);
    flashpane::uncover(&mut host, &mut panes);
    assert!(!panes[0]);
    flashpane::cover(&mut host, &mut panes, 0);
    flashpane::cover(&mut host, &mut panes, 1);
    assert_eq!((panes[0], panes[1]), (false, true));
}

#[test]
fn reopening_takes_back_only_a_flash_card() {
    let mut transfer = flashpane::Handed { id: 7, flash: true, running: true };
    let mut sticky = "1.2 GB of 4.0 GB".to_string();
    flashpane::reclaim(&mut transfer, &mut sticky);
    assert!(!transfer.running);
    assert_eq!(sticky, "");
    let mut copy = flashpane::Handed { id: 8, flash: false, running: true };
    let mut kept = "copying".to_string();
    flashpane::reclaim(&mut copy, &mut kept);
    assert_eq!((copy, kept.as_str()), (flashpane::Handed { id: 8, flash: false, running: true }, "copying"));
}

#[test]
fn done_answers_the_press_just_sent() {
    use super::flashpane::DoneFor;
    assert_eq!(flashpane::done_for(false, 0, true, 5, 5), DoneFor::Refused);
    assert_eq!(flashpane::done_for(false, 0, true, 5, 4), DoneFor::Ignore);
    assert_eq!(flashpane::done_for(true, 6, false, 6, 6), DoneFor::Finished);
    assert_eq!(flashpane::done_for(true, 6, false, 6, 7), DoneFor::Ignore);
}

#[test]
fn moves_choose_only_fitting_drives() {
    let (a, b) = (stick("sda", 2, "AAA", "Alpha", ""), stick("sdb", 3, "BBB", "Bravo", ""));
    let small = stick("sdb", 3, "BBB", "Bravo", "small");
    assert_eq!(flashpane::drive_at(1, &[a.clone(), b.clone()]), Some(a.clone()));
    assert_eq!(flashpane::drive_at(2, &[a.clone(), small]), None);
    assert_eq!(flashpane::drive_at(0, &[a.clone(), b.clone()]), None);
    assert_eq!(flashpane::drive_at(3, &[a.clone(), b]), None);
    assert_eq!(flashpane::choose_kept(1, std::slice::from_ref(&a), None), Some(a.clone()));
    assert_eq!(flashpane::choose_kept(3, std::slice::from_ref(&a), Some(a.clone())), Some(a));
}

#[test]
fn endings_figures_read_off_the_cards_total() {
    let i18n = english();
    let known = Transfer { bytes: 1200000000, total: 4000000000, known: true, permille: 300, ..Transfer::default() };
    let figures = flashpane::ending_context(&known, "cancelled", 0);
    assert_eq!((figures.at, figures.total), (1200000000, 4000000000));
    let ctx = |figures: flashpane::Figures| flashendings::Ctx {
        image: "debian-13.iso".into(),
        drive: "128GB".into(),
        at: figures.at,
        total: figures.total,
        ..flashendings::Ctx::default()
    };
    assert_eq!(
        flashendings::ending_for(&i18n, "cancelled", &ctx(figures)).headline,
        "Flash cancelled at 1.2 GB of 4.0 GB"
    );
    let packed = Transfer { bytes: 1200000000, total: 400000000, known: false, permille: 300, ..Transfer::default() };
    assert_eq!(
        flashendings::ending_for(&i18n, "cancelled", &ctx(flashpane::ending_context(&packed, "cancelled", 0))).headline,
        "Flash cancelled at 1.2 GB of 4.0 GB"
    );
    assert_eq!(flashpane::ending_context(&known, "verify", 2300000000).at, 2300000000);
}

#[test]
fn every_ending_is_a_row_with_its_fix() {
    let i18n = english();
    let ctx = |err: &str| {
        flashendings::ending_for(
            &i18n,
            err,
            &flashendings::Ctx {
                image: "debian-13.iso".into(),
                drive: "128GB".into(),
                at: 1200000000,
                total: 4000000000,
                ..flashendings::Ctx::default()
            },
        )
    };
    let done = flashendings::ending_for(
        &i18n,
        "",
        &flashendings::Ctx { image: "debian-13.iso".into(), drive: "128GB".into(), ..flashendings::Ctx::default() },
    );
    assert_eq!((done.headline.as_str(), done.hint.as_str(), done.eject, done.flash_back), ("Flashed debian-13.iso to 128GB", "", true, false));
    assert_eq!(ctx("cancelled").headline, "Flash cancelled at 1.2 GB of 4.0 GB");
    assert_eq!(ctx("cancelled").hint, "128GB is blank now. Flash again, or format it in Disks.");
    assert!(ctx("cancelled").flash_back);
    assert_eq!(ctx("denied").headline, "Nothing was written");
    assert_eq!(ctx("busy").headline, "128GB could not be unmounted");
    assert_eq!(ctx("changed").headline, "128GB changed since you chose it");
    assert_eq!(ctx("gone").headline, "128GB was unplugged");
    assert_eq!(ctx("gone").hint, "It left at 1.2 GB. Plug it in again and flash again.");
    assert_eq!(ctx("io").headline, "Could not flash 128GB");
    assert_eq!(ctx("verify").headline, "128GB did not verify");
    assert_eq!(ctx("verify").hint, "It read back different data at 1.2 GB. Flash again, or use another drive.");
    let image = |err: &str, image: &str| {
        flashendings::ending_for(
            &i18n,
            err,
            &flashendings::Ctx { image: image.into(), drive: "128GB".into(), drive_bytes: 124623257600, ..flashendings::Ctx::default() },
        )
    };
    assert_eq!(image("nospace", "archlinuxarm.img.gz").headline, "archlinuxarm.img.gz is larger than 128GB");
    assert_eq!(image("decompress", "raspios-arm64.img.xz").headline, "Could not unpack raspios-arm64.img.xz");
    assert_eq!(image("sandbox", "x.img").headline, "Compressed images need bubblewrap");
    let unknown = flashendings::ending_for(
        &i18n,
        "cancelled",
        &flashendings::Ctx { image: "debian-13.iso".into(), drive: "128GB".into(), at: 1200000000, ..flashendings::Ctx::default() },
    );
    assert_eq!(unknown.headline, "Flash cancelled at 1.2 GB");
}

#[test]
fn status_lines_report_as_a_copy_does() {
    let i18n = english();
    let ctx = flashendings::Ctx { image: "debian-13.iso".into(), drive: "128GB".into(), ..flashendings::Ctx::default() };
    let done = flashendings::status_for(&i18n, "", &ctx);
    assert_eq!((done.text.as_str(), done.is_error), ("Flashed debian-13.iso to 128GB.", false));
    let verify = flashendings::status_for(&i18n, "verify", &ctx);
    assert_eq!((verify.text.as_str(), verify.is_error), ("128GB did not verify; flash it again.", true));
    let cancelled = flashendings::Ctx { drive: "128GB".into(), at: 1, total: 2, ..flashendings::Ctx::default() };
    assert!(!flashendings::status_for(&i18n, "cancelled", &cancelled).is_error);
    assert_eq!(flashendings::eject_done_line(&i18n, "128GB"), "Ejected 128GB. It is safe to unplug.");
    assert_eq!(flashendings::eject_busy_line(&i18n, "128GB"), "128GB is still mounted; close what is using it.");
    assert_eq!(flashendings::eject_failed_line(&i18n, "128GB"), "Could not eject 128GB.");
}

#[test]
fn image_struct_defaults_empty() {
    let image = Image::default();
    assert!(image.path.is_empty() && !image.refused);
}
