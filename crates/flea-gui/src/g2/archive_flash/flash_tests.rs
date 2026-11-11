//! The flash view as the board draws it: rows, steps, the run and where the keyboard parks.
use super::flash::{Flash, Seat};
use super::keys::Key;
use super::tests::english;
use flea::jsondoc::parse;
use flea::ui_logic_archive_flash::wire;

fn targets() -> Vec<wire::TargetRow> {
    vec![
        wire::TargetRow {
            dev: "sda".into(),
            seq: 2,
            serial: "2003007157171692".into(),
            label: "128GB".into(),
            model: "General USB Flash Disk".into(),
            bytes: 124623257600,
            refused: String::new(),
        },
        wire::TargetRow {
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

fn opened_with(rows: Vec<wire::TargetRow>) -> Flash {
    let mut view = Flash::closed();
    view.open_with_image("/scope/iso/debian-13.iso");
    view.receive_image(
        "/scope/iso/debian-13.iso",
        &wire::FlashImage {
            kind: "iso".into(),
            bytes: 4294967296,
            unpacked: 4294967296,
            known: true,
            member: String::new(),
        },
    );
    view.receive_targets(rows);
    view
}

fn opened() -> Flash {
    opened_with(targets())
}

#[test]
fn flash_board_names_image_drives_and_step() {
    let i18n = english();
    let view = opened();
    assert!(view.live());
    let drawn = view.view(&i18n);
    assert_eq!(drawn.title.as_str(), "Flash image");
    assert_eq!(drawn.image.name.as_str(), "debian-13.iso");
    assert_eq!(drawn.image.size.as_str(), "4.3 GB");
    assert_eq!(drawn.drives.len(), 2);
    assert_eq!(drawn.drives[0].name.as_str(), "128GB");
    assert_eq!(drawn.drives[0].detail.as_str(), "General USB Flash Disk");
    assert_eq!(drawn.drives[0].size.as_str(), "124.6 GB");
    assert!(!drawn.drives[0].dimmed);
    assert!(drawn.drives[1].dimmed);
    assert_eq!(
        drawn.drives[1].hint.as_str(),
        "It is too small for the image."
    );
    assert_eq!(drawn.step.line.as_str(), "Erases everything on 128GB");
    assert_eq!(drawn.step.action.as_str(), "Flash 128GB");
    assert!(drawn.step.live);
    assert_eq!(drawn.status.as_str(), "2 drives");
}

#[test]
fn one_fitting_drive_is_chosen_for_you() {
    let view = opened();
    assert_eq!(view.selected(), Some(0));
}

#[test]
fn flash_press_sends_identity_and_starts() {
    let i18n = english();
    let mut view = opened();
    let effects = view.press_flash();
    assert_eq!(effects.len(), 1);
    let line = match &effects[0] {
        super::Effect::Send(line) => line.clone(),
        _ => panic!("send"),
    };
    assert!(line.contains(r#""dev":"sda""#), "{line}");
    assert!(line.contains(r#""serial":"2003007157171692""#), "{line}");
    view.start_transfer(&i18n, 1, 4294967296, true, 0);
    assert!(view.running());
}

#[test]
fn failed_flash_parks_the_keyboard_on_the_end_button() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    let done = parse(
        r#"{"t":"flashdone","id":1,"ok":false,"phase":"write","at":0,"err":"busy","blank":false}"#,
    )
    .unwrap();
    view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    assert!(!view.running());
    let ending = view.ending().expect("ending");
    assert_eq!(ending.headline.as_str(), "128GB could not be unmounted");
    assert!(ending.flash_back);
    assert_eq!(view.park(), Seat::End);
    let drawn = view.view(&i18n);
    assert!(drawn.step.end_visible);
    assert_eq!(drawn.step.end_label.as_str(), "Flash 128GB");
    assert!(drawn.step.end_primary);
}

#[test]
fn done_flash_offers_eject_alone() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    let done = parse(
        r#"{"t":"flashdone","id":1,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#,
    )
    .unwrap();
    view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    let ending = view.ending().expect("ending");
    assert_eq!(ending.headline.as_str(), "Flashed debian-13.iso to 128GB");
    assert!(ending.eject && !ending.flash_back);
    let drawn = view.view(&i18n);
    assert_eq!(drawn.step.end_label.as_str(), "Eject");
    assert!(!drawn.step.end_primary);
}

#[test]
fn eject_lands_and_parks_on_the_pane() {
    let i18n = english();
    let mut view = opened();
    let effects = view.eject_chosen();
    assert_eq!(effects.len(), 1);
    let eject = parse(r#"{"t":"flasheject","ok":true,"err":""}"#).unwrap();
    let said = view.finish_eject(&i18n, &wire::parse_flasheject(&eject));
    assert_eq!(said.len(), 1);
    assert_eq!(view.park(), Seat::Pane);
}

#[test]
fn escape_leaves_and_never_cancels() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    let effects = view.key(Key::Escape);
    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], super::Effect::HandTransfer(_)));
    assert!(!view.shown());
    assert!(view.running());
}

#[test]
fn the_step_tap_on_a_dimmed_flush_cancel_sends_nothing() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 4294967296, true, 0);
    view.progress_transfer(
        &i18n,
        &wire::FlashProgress {
            id: 1,
            phase: "flush".into(),
            bytes: 4294967296,
            total: 4294967296,
            permille: 1000,
        },
        10,
    );
    assert!(view.running(), "the flash still runs in its flush");
    assert!(
        !view.view(&i18n).step.cancel_live,
        "the flush dims Cancel (Transfer.js cancelEnabled)"
    );
    assert!(
        view.press_step().is_empty(),
        "a tap on a dimmed Cancel sends no flashcancel"
    );
}

#[test]
fn flash_rows_take_pointer_choices_with_the_oracles_guards() {
    let i18n = english();
    let mut view = opened();
    assert!(
        view.choose_image()
            .iter()
            .any(|e| matches!(e, super::Effect::PickImage)),
        "an image row click asks for the picker"
    );
    view.choose_drive(1);
    assert_eq!(
        view.selected(),
        Some(0),
        "a refused drive never becomes the choice"
    );
    view.choose_drive(0);
    assert_eq!(view.selected(), Some(0));
    view.choose_drive(99);
    assert_eq!(
        view.selected(),
        Some(0),
        "an out-of-range row chooses nothing"
    );
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    assert!(
        view.choose_image().is_empty(),
        "no picker while a flash runs"
    );
    // Two fitting drives with the second chosen, so a choice while running would visibly move.
    let mut rows = targets();
    rows[1].refused = String::new();
    let mut raced = opened_with(rows);
    assert_eq!(raced.selected(), None, "two fitting drives preselect none");
    raced.choose_drive(1);
    assert_eq!(raced.selected(), Some(1));
    raced.press_flash();
    raced.start_transfer(&i18n, 1, 1000, true, 0);
    raced.choose_drive(0);
    assert_eq!(raced.selected(), Some(1), "no choice while a flash runs");
    let drawn = raced.view(&i18n);
    assert!(
        !drawn.drives[0].cursor && !drawn.drives[1].cursor,
        "the cursor never leaves the Cancel step mid-flash"
    );
    assert!(
        drawn.step.cursor,
        "Cancel keeps the keyboard while the flash runs"
    );
}

#[test]
fn cursor_walk_skips_the_refused_row() {
    let mut view = opened();
    view.move_cursor(1);
    assert_eq!(view.selected(), Some(0));
    view.move_cursor(1);
    assert_eq!(
        view.selected(),
        Some(0),
        "the refused drive never becomes the choice"
    );
}

#[test]
fn flash_enter_on_the_flash_step_presses() {
    let mut view = opened();
    view.move_cursor(1);
    assert_eq!(view.park(), Seat::Flash);
    assert!(view.live());
    let effects = view.key(Key::Enter);
    assert_eq!(effects.len(), 1, "Enter on the Flash step flashes");
    assert!(matches!(&effects[0], super::Effect::Send(line) if line.contains(r#""c":"flash""#)));
}

#[test]
fn flash_enter_on_the_cancel_step_cancels() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    assert_eq!(view.park(), Seat::Cancel);
    let effects = view.key(Key::Space);
    assert_eq!(effects.len(), 1, "Space on Cancel cancels");
    assert!(matches!(&effects[0], super::Effect::Send(line) if line.contains("flashcancel")));
}

#[test]
fn flash_enter_on_the_end_button_presses_it() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 1, 1000, true, 0);
    let done = parse(
        r#"{"t":"flashdone","id":1,"ok":false,"phase":"write","at":0,"err":"busy","blank":false}"#,
    )
    .unwrap();
    view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    assert_eq!(view.park(), Seat::End);
    let effects = view.key(Key::Enter);
    assert!(
        matches!(effects.as_slice(), [super::Effect::Send(line)] if line.contains(r#""c":"flash""#)),
        "Enter on the busy ending's button flashes again, never ejects: {effects:?}"
    );
}

#[path = "flash_ending_tests.rs"]
mod ending;
