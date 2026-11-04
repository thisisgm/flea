//! Flash endings, refusal and reopen transitions.
use super::*;

#[test]
fn flash_finished_after_back_reclaims_the_handed_job() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    view.start_transfer(&i18n, 7, 1000, true, 0);
    let handed = view.key(Key::Escape);
    assert!(handed
        .iter()
        .any(|e| matches!(e, super::super::Effect::HandTransfer(_))));
    let done = parse(
        r#"{"t":"flashdone","id":7,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#,
    )
    .unwrap();
    let effects = view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    assert_eq!(
        effects,
        vec![
            super::super::Effect::CloseJob(7),
            super::super::Effect::Say {
                text: "Flashed debian-13.iso to 128GB.".into(),
                error: false
            },
        ],
        "the handed block closes by id before the status line"
    );
}

#[test]
fn step_action_hover_and_press_ride_the_view() {
    let i18n = english();
    let mut view = opened();
    assert!(!view.view(&i18n).step.action_hover && !view.view(&i18n).step.action_pressed);
    view.set_action_hover(true, std::time::Instant::now());
    view.set_action_pressed(true, std::time::Instant::now());
    let drawn = view.view(&i18n);
    assert!(
        drawn.step.action_hover && drawn.step.action_pressed,
        "the pointer states draw locally"
    );
    view.set_action_hover(false, std::time::Instant::now());
    view.set_action_pressed(false, std::time::Instant::now());
    assert!(!view.view(&i18n).step.action_hover && !view.view(&i18n).step.action_pressed);
}

#[test]
fn step_plan_names_cancel_bar_and_end_button() {
    use super::super::flash::{StepAction, StepMark};
    let i18n = english();
    let view = opened();
    let rest = view.view(&i18n);
    assert_eq!(
        rest.step.step_action(),
        StepAction::Flash,
        "rest draws the Flash action"
    );
    assert!(!rest.step.show_bar());
    assert_eq!(rest.image.mark(), super::super::cards::Glyph::File);
    assert!(rest.image.hint.is_empty());
    assert!(rest.focused, "the shown flash holds the keys");
    let mut running = opened();
    running.press_flash();
    running.start_transfer(&i18n, 1, 1000, true, 0);
    let drawn = running.view(&i18n);
    assert_eq!(
        drawn.step.step_action(),
        StepAction::Cancel,
        "running draws Cancel"
    );
    assert!(
        drawn.step.show_bar(),
        "running draws the bar and the byte line"
    );
    let mut ended = opened();
    ended.press_flash();
    ended.start_transfer(&i18n, 1, 1000, true, 0);
    let done = parse(
        r#"{"t":"flashdone","id":1,"ok":false,"phase":"write","at":0,"err":"busy","blank":false}"#,
    )
    .unwrap();
    ended.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    let over = ended.view(&i18n);
    assert_eq!(
        over.step.step_action(),
        StepAction::End,
        "a failure draws its end button"
    );
    assert!(over.step.end_visible && over.step.end_primary);
    assert_eq!(over.step.mark, StepMark::Alert);
}

#[test]
fn refused_image_names_its_hint_and_picker_takes_the_folder_mark() {
    use super::super::flash::Flash;
    let i18n = english();
    let mut view = Flash::closed();
    view.open_with_image("/scope/iso/notes.txt");
    view.receive_image(
        "/scope/iso/notes.txt",
        &wire::FlashImage {
            kind: String::new(),
            bytes: 10,
            unpacked: 0,
            known: false,
            member: String::new(),
        },
    );
    let drawn = view.view(&i18n);
    assert!(drawn.image.refused);
    assert_eq!(drawn.image.hint.as_str(), "This file is not a disk image.");
    let mut bare = Flash::closed();
    bare.open();
    let picker = bare.view(&i18n);
    assert_eq!(
        picker.image.mark(),
        super::super::cards::Glyph::FolderOpen,
        "the picker row takes the folder mark"
    );
    assert_eq!(picker.image.name.as_str(), "Choose an image…");
}

#[test]
fn probe_refusal_reasks_the_image_row() {
    let i18n = english();
    let mut view = opened();
    view.press_flash();
    let done = parse(
        r#"{"t":"flashdone","id":1,"ok":false,"phase":"probe","at":0,"err":"io","blank":false}"#,
    )
    .unwrap();
    let effects = view.finish_transfer(&i18n, &wire::parse_flashdone(&done));
    assert_eq!(effects.len(), 1);
    assert!(matches!(&effects[0], super::super::Effect::Send(line) if line.contains("flashimage")));
}

#[test]
fn a_dead_backend_ends_a_running_flash_with_a_failure() {
    let i18n = english();
    let mut shown = opened();
    shown.press_flash();
    shown.start_transfer(&i18n, 1, 1000, true, 0);
    assert!(
        shown.on_failed(&i18n).is_empty(),
        "a shown card draws its ending instead of a status line"
    );
    assert!(!shown.running(), "the dead run no longer counts as running");
    let mut handed = opened();
    handed.press_flash();
    handed.start_transfer(&i18n, 1, 1000, true, 0);
    assert!(matches!(
        handed.key(Key::Escape).as_slice(),
        [super::super::Effect::HandTransfer(_)]
    ));
    let effects = handed.on_failed(&i18n);
    assert!(
        matches!(effects.as_slice(), [super::super::Effect::CloseJob(1), super::super::Effect::Say { text, error: true }] if text.starts_with("Could not flash")),
        "a handed run closes its job and says the failure: {effects:?}"
    );
    assert!(!handed.running());
    assert!(
        opened().on_failed(&i18n).is_empty(),
        "with nothing running only the press ends"
    );
}

#[test]
fn a_reopen_while_the_press_is_in_flight_keeps_its_image_and_drive() {
    let i18n = english();
    let mut view = opened();
    assert!(!view.press_flash().is_empty(), "the press is sent");
    assert_eq!(
        view.key(Key::Escape),
        [super::super::Effect::Close],
        "Escape before flashstarted only hides the card"
    );
    assert!(
        view.open_with_image("/d/other.iso").is_empty(),
        "a reopen onto another image sends nothing while the press is in flight"
    );
    assert!(
        view.open_with_drive("sdz").is_empty(),
        "a reopen onto another drive sends nothing while the press is in flight"
    );
    view.start_transfer(&i18n, 1, 1000, true, 0);
    assert_eq!(
        view.view(&i18n).image.name,
        "debian-13.iso",
        "the running erase keeps the image it was pressed with"
    );
    assert_eq!(
        view.selected(),
        Some(0),
        "and the drive it was pressed with"
    );
    let handed = view.key(Key::Escape);
    assert!(
        matches!(handed.as_slice(), [super::super::Effect::HandTransfer(block)] if block.head.contains("128GB")),
        "the handed run names the pressed drive: {handed:?}"
    );
}
