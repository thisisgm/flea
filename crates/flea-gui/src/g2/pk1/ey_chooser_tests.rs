// Chooser defects from the eyes sweep, each held to what the v0.3.12 arm does in the same steps.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use flea::ui_logic_pl4::rail::{Group, Kind, RailEntry};
use gpui::{KeyUpEvent, Keystroke, TestAppContext};

// Past the row's side padding and place glyph, so a click lands on the label and not the trailing mark.
const LABEL_X: f32 = 30.;

fn seed(f: &mut Fixture) {
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":2,"v":1,"w":true,"path":"/d"}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"b.md","d":false,"s":7,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
}

// A real key is pressed and released; gpui's keyboard click fires on the release of Enter or Space.
fn press(f: &mut Fixture, key: &str) {
    f.cx.simulate_keystrokes(key);
    f.cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).unwrap(),
    });
    f.cx.run_until_parked();
}

#[gpui::test]
fn ey500_a_keyboard_pick_closes_the_filter_menu(cx: &mut TestAppContext) {
    for opener in ["enter", "space", "down"] {
        let mut f = fixture(
            cx,
            r#"{"folder":"/d","filters":[{"label":"Text","globs":["*.txt"]},{"label":"Markdown","globs":["*.md"]}]}"#,
            None,
        );
        f.cx.run_until_parked();
        seed(&mut f);
        press(&mut f, "tab");
        press(&mut f, opener);
        assert!(
            f.cx.debug_bounds("picker-filter-menu").is_some(),
            "{opener} on the trigger opens the Filter menu"
        );
        press(&mut f, "down");
        press(&mut f, "enter");
        assert_eq!(f.read(|picker| picker.listing.chip), 1, "{opener}");
        assert!(
            f.read(|picker| picker.chrome.menu_view.is_none()),
            "{opener}: the pick closes the menu"
        );
        assert!(
            f.cx.debug_bounds("picker-filter-menu").is_none(),
            "{opener}: no Filter menu is drawn after the pick"
        );
    }
}

#[gpui::test]
fn ey497_a_mounted_removable_drive_ends_in_the_eject_mark(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    let drive = |label: &str, releasable: bool| RailEntry {
        group: Group::Device,
        kind: Kind::Volume,
        label: label.into(),
        path: format!("/run/media/{label}"),
        device: format!("/dev/{label}"),
        mounted: Some(true),
        releasable,
        glyph: "drive",
        ..RailEntry::default()
    };
    f.update(|picker, cx| {
        picker.rail.entries = vec![drive("FLEAEXFAT", true), drive("vda2", false)];
        picker.rail.labels = vec!["FLEAEXFAT".into(), "vda2".into()];
        cx.notify();
    });
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-rail-eject-0").is_some(),
        "a mounted removable drive ends in the eject mark"
    );
    assert!(
        f.cx.debug_bounds("picker-rail-dot-0").is_none(),
        "the eject mark stands in the square's own slot"
    );
    assert!(
        f.cx.debug_bounds("picker-rail-dot-1").is_some()
            && f.cx.debug_bounds("picker-rail-eject-1").is_none(),
        "a fixed drive keeps its mounted square"
    );
    let before = f.read(|picker| picker.listing.location.clone());
    let mark = f.cx.debug_bounds("picker-rail-eject-0").unwrap().center();
    f.cx.simulate_click(mark, gpui::Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        before,
        "a click on the eject mark opens nothing, as SidebarRow.qml's row tap skips it in the chooser"
    );
    let row = f.cx.debug_bounds("picker-rail-row-0").unwrap();
    let label = row.origin + gpui::point(gpui::px(LABEL_X), row.size.height / 2.);
    f.cx.simulate_click(label, gpui::Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/run/media/FLEAEXFAT",
        "the rest of the row still opens the drive"
    );
}

// EY-509: PickerWindow.qml onChosen calls focusDefault, so a place opened from the rail hands the keyboard to the files.
#[gpui::test]
fn ey509_a_click_on_recent_from_the_rail_moves_the_keyboard_to_the_files(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.run_until_parked();
    seed(&mut f);
    let rail = f.read(|picker| picker.rail.focus.clone());
    f.cx.update(|window, cx| window.focus(&rail, cx));
    f.cx.run_until_parked();
    let recent = f.cx.debug_bounds("picker-rail-row-0").unwrap().center();
    f.cx.simulate_click(recent, gpui::Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        flea::ui_logic_pk1::RECENT
    );
    let picker = f.picker.clone();
    assert!(
        f.cx.update(|window, cx| {
            let picker = picker.read(cx);
            picker.focus.is_focused(window) && !picker.rail.focus.is_focused(window)
        }),
        "the files take the keyboard after a rail choice, as picker-recent-list draws"
    );
}

// Enter on a rail folder hands the keyboard on as PickerWindow.qml focusDefault does: the files, or Filename in a save.
#[gpui::test]
fn ey509_enter_on_a_rail_folder_hands_the_keyboard_to_the_default_field(cx: &mut TestAppContext) {
    for save in [false, true] {
        let request = if save {
            r#"{"mode":"save","name":"untitled.txt","folder":"/d"}"#
        } else {
            r#"{"folder":"/d"}"#
        };
        let mut f = fixture(cx, request, None);
        f.cx.run_until_parked();
        seed(&mut f);
        let rail = f.read(|picker| picker.rail.focus.clone());
        f.cx.update(|window, cx| window.focus(&rail, cx));
        f.update(|picker, cx| {
            picker.rail.cursor = picker
                .rail
                .entries
                .iter()
                .position(|entry| entry.path == "/home/test")
                .expect("the rail offers Home");
            cx.notify();
        });
        f.cx.run_until_parked();
        press(&mut f, "enter");
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            "/home/test",
            "save {save}"
        );
        let picker = f.picker.clone();
        let (files, filename, on_rail) = f.cx.update(|window, cx| {
            let picker = picker.read(cx);
            let filename = picker
                .browse
                .filename
                .as_ref()
                .is_some_and(|field| field.read(cx).focus.is_focused(window));
            (
                picker.focus.is_focused(window),
                filename,
                picker.rail.focus.is_focused(window),
            )
        });
        assert!(!on_rail, "save {save}: the rail gives up the keyboard");
        assert_eq!(
            (files, filename),
            (!save, save),
            "the files in an open, Filename in a save"
        );
    }
}
