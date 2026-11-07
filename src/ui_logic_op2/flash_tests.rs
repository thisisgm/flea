//! Port of tests/js/flash.js: refusal hints, names, cursor, words, endings and lines (image kinds live in mn1).
use super::flash::*;
use super::{english, flashcard};
use crate::ui_logic_mn1::rows::{image_kind, is_disk_image};

#[test]
fn image_kinds_read_through_mn1() {
    assert_eq!(image_kind("debian-13.iso"), "iso");
    assert_eq!(image_kind("disk.img"), "img");
    assert_eq!(image_kind("disk.raw"), "raw");
    assert_eq!(image_kind("raspios-arm64.img.xz"), "xz");
    assert_eq!(image_kind("archlinuxarm.img.gz"), "gz");
    assert_eq!(image_kind("disk.img.zst"), "zst");
    assert_eq!(image_kind("disk.img.bz2"), "bz2");
    assert_eq!(image_kind("DEBIAN-13.ISO"), "iso");
    assert_eq!(image_kind("notes.txt"), "");
    assert!(is_disk_image("debian-13.iso", ""));
    assert!(is_disk_image("disk.img", ""));
    assert!(is_disk_image("disk.raw", ""));
    assert!(is_disk_image("raspios-arm64.img.xz", ""));
    assert!(is_disk_image("archlinuxarm.img.gz", ""));
    assert!(is_disk_image("disk.img.zst", ""));
    assert!(is_disk_image("disk.img.bz2", ""));
    assert!(is_disk_image("DEBIAN-13.ISO", ""));
    assert!(is_disk_image("bundle.zip", "disk.img"));
    assert!(!is_disk_image("notes.txt", ""));
    assert!(!is_disk_image("archive.zip", ""));
    assert!(!is_disk_image("bundle.zip", ""));
    assert!(!is_disk_image("bundle.zip", "notes.txt"));
    assert!(!is_disk_image("bundle.zip", "inner.zip"));
}

fn drives() -> Vec<Drive> {
    vec![
        Drive {
            dev: "sda".into(),
            seq: 2,
            serial: "2003007157171692".into(),
            label: "128GB".into(),
            model: "General USB Flash Disk".into(),
            bytes: 124623257600,
            refused: String::new(),
        },
        Drive {
            dev: "sdb".into(),
            seq: 3,
            serial: "001122334455".into(),
            label: "SanDisk".into(),
            model: "SanDisk Cruzer Blade".into(),
            bytes: 2000000000,
            refused: "small".into(),
        },
    ]
}

#[test]
fn refusal_hints_use_the_boards_wording() {
    let i18n = english();
    let rows = drives();
    assert!(fits(&rows[0]));
    assert!(!fits(&rows[1]));
    assert_eq!(i18n.tr(refused_hint_key(&rows[1])), "It is too small for the image.");
    assert_eq!(
        i18n.tr(refused_hint_key(&Drive { dev: "mmcblk0".into(), refused: "readonly".into(), ..Drive::default() })),
        "Its lock switch is on. Slide the switch off and plug the card in again."
    );
    assert_eq!(
        i18n.tr(refused_hint_key(&Drive { dev: "sda".into(), refused: "readonly".into(), ..Drive::default() })),
        "It is read-only, so Flea cannot write to it."
    );
    assert_eq!(
        i18n.tr(refused_hint_key(&Drive { dev: "sda".into(), refused: "holdsimage".into(), ..Drive::default() })),
        "The image is on this drive. Copy it elsewhere first."
    );
    assert_eq!(
        i18n.tr(refused_hint_key(&Drive { dev: "sda".into(), refused: "member".into(), ..Drive::default() })),
        "It is part of a RAID or volume group."
    );
}

#[test]
fn names_keep_label_model_and_kernel_name() {
    let rows = drives();
    assert_eq!(drive_name(&rows[0], &rows), "128GB");
    assert_eq!(drive_detail(&rows[0]), "General USB Flash Disk");
    let twins = vec![
        Drive { dev: "sda".into(), label: "128GB".into(), model: "General USB Flash Disk".into(), ..Drive::default() },
        Drive { dev: "sdb".into(), label: "128GB".into(), model: "Kingston DataTraveler".into(), ..Drive::default() },
    ];
    assert_eq!(drive_name(&twins[1], &twins), "128GB (sdb)");
    let single = [Drive { dev: "sdc".into(), model: "Seagate Expansion".into(), ..Drive::default() }];
    assert_eq!(drive_detail(&single[0]), "");
    let unlabeled = vec![
        Drive { dev: "sda".into(), model: "Seagate Expansion".into(), ..Drive::default() },
        Drive { dev: "sdb".into(), model: "Seagate Expansion".into(), ..Drive::default() },
    ];
    assert_eq!(drive_name(&unlabeled[0], &unlabeled), "Seagate Expansion (sda)");
    assert_eq!(drive_detail(&unlabeled[0]), "", "an unlabeled twin never repeats its model as the detail");
}

#[test]
fn one_fitting_drive_is_preselected() {
    let rows = drives();
    assert_eq!(preselect(&rows), Some(0));
    let two = vec![rows[0].clone(), Drive { dev: "sdc".into(), label: "Backup".into(), ..Drive::default() }];
    assert_eq!(preselect(&two), None);
    assert_eq!(preselect(&rows[1..]), None);
}

#[test]
fn cursor_walk_skips_refused_rows() {
    let rows = drives();
    assert_eq!(step(0, 1, &rows, true), 1);
    assert_eq!(step(1, 1, &rows, true), 3);
    assert_eq!(step(3, -1, &rows, true), 1);
    assert_eq!(step(3, 1, &rows, true), 3);
    assert_eq!(flash_pos(2), 3);
}

#[test]
fn the_image_row_is_a_stop_before_an_image_is_chosen() {
    let rows = drives();
    // Enter on row 0 opens the chooser, so Up from the drive reaches it with no image chosen yet.
    assert_eq!(step(1, -1, &rows, true), 0);
}

#[test]
fn flash_arms_only_on_image_and_fitting_drive() {
    let i18n = english();
    let rows = drives();
    assert!(flash_live(true, Some(&rows[0])));
    assert!(!flash_live(false, Some(&rows[0])));
    assert!(!flash_live(true, Some(&rows[1])));
    assert!(!flash_live(true, None));
    let words = flash_words(&i18n, true, Some(&rows[0]), &rows);
    assert_eq!(words.erase, "Erases everything on 128GB");
    assert_eq!(words.action, "Flash 128GB");
    let bare = flash_words(&i18n, false, None, &rows);
    assert_eq!(bare.erase, "Erases everything on the drive");
    assert_eq!(bare.action, "Flash");
}

#[test]
fn status_counts_drives_and_groups() {
    let i18n = english();
    assert_eq!(drives_line(&i18n, 1), "1 drive");
    assert_eq!(drives_line(&i18n, 2), "2 drives");
    assert_eq!(drives_line(&i18n, 1204), "1,204 drives");
    assert_eq!(already_running(&i18n), "A flash is already running. Wait for it to finish.");
    assert_eq!(up_unavailable(&i18n), "Up unavailable in Flash");
}

#[test]
fn rail_volumes_name_their_whole_disk() {
    assert_eq!(rail_disk("/dev/sda1"), "sda");
    assert_eq!(rail_disk("/dev/nvme0n1p2"), "nvme0n1");
    assert_eq!(rail_disk("/dev/mmcblk0p1"), "mmcblk0");
    assert_eq!(rail_disk("/dev/sda"), "sda");
    assert_eq!(rail_disk("/dev/mmcblk0"), "mmcblk0");
    assert_eq!(rail_disk("/dev/loop0"), "loop0");
    assert_eq!(rail_disk("/dev/loop0p1"), "loop0");
    assert_eq!(rail_disk("/dev/sdb12"), "sdb");
    assert_eq!(rail_disk("/dev/sdp1"), "sdp");
}

#[test]
fn image_detail_names_unpacked_only_for_compressed() {
    let i18n = english();
    let raw = Image { kind: "iso".into(), bytes: 4294967296, unpacked: 4294967296, known: true, ..Image::default() };
    assert_eq!(image_detail(&i18n, &raw), "");
    let packed =
        Image { kind: "xz".into(), bytes: 1200000000, unpacked: 5900000000, known: true, ..Image::default() };
    assert_eq!(image_detail(&i18n, &packed), "5.9 GB unpacked");
    let unknown =
        Image { kind: "gz".into(), bytes: 812400000, unpacked: 0, known: false, ..Image::default() };
    assert_eq!(image_detail(&i18n, &unknown), "");
}

#[test]
fn image_reply_lands_only_on_the_cards_own_image() {
    let shown = Image { path: "/d/a.img.xz".into(), ..Image::default() };
    let mine = image_from(Some(&shown), "/d/a.img.xz", "xz", 10, 20, true).unwrap();
    assert_eq!((mine.kind.as_str(), mine.unpacked, mine.refused), ("xz", 20, false));
    assert!(image_from(Some(&shown), "/d/other.zip", "zip", 5, 0, false).is_none());
    assert!(image_from(None, "/d/other.zip", "zip", 5, 0, false).is_none());
    let refused = image_from(Some(&shown), "/d/a.img.xz", "", 0, 0, false).unwrap();
    assert!(refused.refused);
}

#[test]
fn step_role_wears_the_warning_red_at_rest() {
    assert_eq!(step_role(false, None), "error");
    assert_eq!(step_role(true, None), "foreground");
    assert_eq!(step_role(false, Some("error")), "error");
    assert_eq!(step_role(false, Some("foreground")), "foreground");
}

#[test]
fn card_renders_a_known_total_as_a_copy_does() {
    let i18n = english();
    let flashing = flashcard::Transfer {
        phase: "write".into(),
        drive: "128GB".into(),
        name: "debian-13.iso".into(),
        bytes: 1200000000,
        total: 4000000000,
        known: true,
        permille: 300,
    };
    let drawn = flashcard::render(&i18n, &flashing, 38200000);
    assert_eq!(drawn.head_text, "Flashing 128GB");
    let text: String = drawn.byte_text.iter().map(|part| part.text.clone()).collect();
    assert_eq!(text, "1.2 GB of 4.0 GB · 38.2 MB/s · 1:13 left");
    assert!((drawn.share - 0.3).abs() < 1e-9);
    assert!(!drawn.no_cancel);
    let verifying = flashcard::Transfer { phase: "verify".into(), ..flashing.clone() };
    assert_eq!(flashcard::render(&i18n, &verifying, 0).head_text, "Verifying 128GB");
}

#[test]
fn card_counts_an_unknown_total_off_the_device() {
    let i18n = english();
    let unknown = flashcard::Transfer {
        phase: "write".into(),
        drive: "128GB".into(),
        name: "debian-13.iso".into(),
        bytes: 2100000000,
        total: 0,
        known: false,
        permille: 500,
    };
    let drawn = flashcard::render(&i18n, &unknown, 36400000);
    let text: String = drawn.byte_text.iter().map(|part| part.text.clone()).collect();
    assert_eq!(text, "2.1 GB written · 36.4 MB/s · 0:57 left");
    assert_eq!(drawn.share, 0.5);
    let stalled = flashcard::render(&i18n, &unknown, 0);
    let flat: String = stalled.byte_text.iter().map(|part| part.text.clone()).collect();
    assert_eq!(flat, "2.1 GB written · 0 B/s");
}

#[test]
fn card_names_waits_and_flushes() {
    let i18n = english();
    let waiting = flashcard::Transfer { phase: "password".into(), ..flashcard::Transfer::default() };
    let text: String =
        flashcard::render(&i18n, &waiting, 0).byte_text.iter().map(|part| part.text.clone()).collect();
    assert_eq!(text, "waiting for your password");
    let flushed = flashcard::Transfer {
        phase: "flush".into(),
        drive: "128GB".into(),
        name: "debian-13.iso".into(),
        bytes: 4000000000,
        total: 4000000000,
        known: true,
        permille: 1000,
    };
    let drawn = flashcard::render(&i18n, &flushed, 0);
    let text: String = drawn.byte_text.iter().map(|part| part.text.clone()).collect();
    assert_eq!(text, "4.0 GB written · waiting for the drive");
    assert!(drawn.no_cancel);
    let nameless = flashcard::Transfer { name: "archlinuxarm.img.gz".into(), bytes: 1, ..flashcard::Transfer::default() };
    assert_eq!(flashcard::file(&i18n, &nameless), "archlinuxarm.img.gz");
    let known = flashcard::Transfer { name: "debian-13.iso".into(), total: 4000000000, ..flashcard::Transfer::default() };
    assert_eq!(flashcard::file(&i18n, &known), "debian-13.iso · 4.0 GB");
}

#[test]
fn huge_backend_figures_saturate_instead_of_wrapping() {
    let i18n = english();
    let huge = flashcard::Transfer {
        phase: "write".into(),
        drive: "128GB".into(),
        name: "debian-13.iso".into(),
        bytes: u64::MAX,
        total: 0,
        known: false,
        permille: 1,
    };
    assert_eq!(flashcard::total(&huge), u64::MAX, "the unpacker share scales up without wrapping");
    let drawn = flashcard::render(&i18n, &huge, u64::MAX);
    assert!(drawn.share <= 1.0, "the bar clamps");
    let over = flashcard::Transfer { known: true, bytes: 5, total: 4, ..huge };
    assert_eq!(flashcard::share(&over), 1.0, "known bytes past the total clamp the bar full");
}
