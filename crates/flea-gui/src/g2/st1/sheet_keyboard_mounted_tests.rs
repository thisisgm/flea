//! The sheet's keyboard mounted in the whole window over the US fixture: chords are looked up, the field keeps editing.
use super::scroll_mounted_tests::window;
use super::scroll_tests::draw;
use super::sheet_view::SheetView;
use crate::window::{root::Root, user_keys::test_fixture::samples};
use gpui::{Entity, KeyDownEvent, KeyUpEvent, Keystroke, VisualTestContext, WindowHandle};

pub(super) struct Open {
    handle: WindowHandle<Root>,
    pub(super) view: Entity<SheetView>,
    pub(super) cx: VisualTestContext,
}

// The sheet as `?` opens it, given the keyboard the platform's samples would build; the test platform samples none.
pub(super) fn opened(width: f32, height: f32) -> Open {
    let (handle, mut cx) = window(width, height);
    cx.simulate_keystrokes("?");
    draw(&mut cx);
    let (view, keyboard) = handle
        .update(&mut cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let view = shell.settings.sheet.clone().expect("sheet opened");
            (view, shell.sheet_keyboard_from("default", false, samples))
        })
        .expect("window open");
    view.update(&mut cx, |view, cx| {
        view.sheet.set_keyboard(keyboard);
        cx.notify();
    });
    draw(&mut cx);
    Open { handle, view, cx }
}

pub(super) fn open(open: &mut Open) -> bool {
    open.handle
        .update(&mut open.cx, |root, _, cx| {
            root.shell.read(cx).settings.sheet.is_some()
        })
        .expect("window open")
}

#[test]
fn a_ctrl_chord_is_looked_up_and_never_run() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-c");
    draw(&mut sheet.cx);
    let chord = sheet.view.read_with(&sheet.cx, |view, _| {
        let keyboard = view.sheet.keyboard.as_ref().expect("a keyboard");
        view.sheet
            .chord
            .map(|cap| keyboard.chord(view.sheet.held, cap))
    });
    assert_eq!(chord.as_deref(), Some("CTRL + C"));
    assert!(open(&mut sheet), "a looked-up chord leaves the sheet open");
    sheet
        .view
        .read_with(&sheet.cx, |view, _| assert!(view.sheet.query.is_empty()));
}

/// What the field holds after `keys`, with or without a keyboard over the sheet; over one, Ctrl+F opens the search first.
fn typed(keyboard: bool, keys: &str) -> String {
    let mut sheet = opened(2560., 1400.);
    if keyboard {
        sheet.cx.simulate_keystrokes("ctrl-f");
    } else {
        sheet
            .view
            .update(&mut sheet.cx, |view, _| view.sheet.set_keyboard(None));
        draw(&mut sheet.cx);
    }
    sheet.cx.simulate_keystrokes(keys);
    draw(&mut sheet.cx);
    sheet
        .view
        .read_with(&sheet.cx, |view, _| view.sheet.query.clone())
}

#[test]
fn the_fields_editing_keys_edit_as_they_do_without_a_keyboard() {
    for keys in [
        "r e n a m e ctrl-backspace",
        "r e n ctrl-left x",
        "r e n home ctrl-delete",
        "r e n ctrl-a ctrl-x",
        "r e n shift-left shift-left z",
    ] {
        assert_eq!(typed(true, keys), typed(false, keys), "{keys}");
    }
}

#[test]
fn a_window_without_room_looks_nothing_up() {
    let mut sheet = opened(700., 480.);
    sheet.cx.simulate_keystrokes("ctrl-c");
    draw(&mut sheet.cx);
    sheet
        .view
        .read_with(&sheet.cx, |view, _| assert_eq!(view.sheet.chord, None));
}

/// The looked-up chord as the corner reads it, and what the field holds.
fn shown(sheet: &Open) -> (Option<String>, String) {
    sheet.view.read_with(&sheet.cx, |view, _| {
        let keyboard = view.sheet.keyboard.as_ref().expect("a keyboard");
        let chord = view
            .sheet
            .chord
            .map(|cap| keyboard.chord(view.sheet.held, cap));
        (chord, view.sheet.query.clone())
    })
}

/// A key's auto-repeat as Wayland sends it: the first press again, typed text and all, marked held.
pub(super) fn repeat(sheet: &mut Open, key: &str) {
    let mut keystroke = Keystroke::parse(key).expect("a key");
    keystroke.key_char = Some(key.into());
    sheet.cx.simulate_event(KeyDownEvent {
        keystroke,
        is_held: true,
        prefer_character_input: false,
    });
    draw(&mut sheet.cx);
}

fn release(sheet: &mut Open, key: &str) {
    let keystroke = Keystroke::parse(key).expect("a key");
    sheet.cx.simulate_event(KeyUpEvent { keystroke });
    draw(&mut sheet.cx);
}

#[test]
fn a_key_that_types_nothing_looks_itself_up_until_it_is_let_go() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("f2");
    draw(&mut sheet.cx);
    assert_eq!(shown(&sheet), (Some("F2".into()), String::new()));
    release(&mut sheet, "f2");
    assert_eq!(shown(&sheet), (None, String::new()));
    sheet.cx.simulate_keystrokes("escape");
    assert!(!open(&mut sheet), "escape still closes the sheet");
}

#[test]
fn a_letter_looks_itself_up_and_types_only_once_the_search_is_open() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("h");
    draw(&mut sheet.cx);
    assert_eq!(shown(&sheet), (Some("H".into()), String::new()));
    repeat(&mut sheet, "h");
    assert_eq!(
        shown(&sheet),
        (Some("H".into()), String::new()),
        "its repeats stay with it"
    );
    release(&mut sheet, "h");
    assert_eq!(shown(&sheet), (None, String::new()));
    sheet.cx.simulate_keystrokes("ctrl-f r e n");
    draw(&mut sheet.cx);
    assert_eq!(shown(&sheet), (None, "ren".into()));
    sheet.cx.simulate_keystrokes("escape");
    draw(&mut sheet.cx);
    assert_eq!(
        shown(&sheet),
        (None, String::new()),
        "escape leaves the search"
    );
    assert!(open(&mut sheet), "and keeps the sheet");
    sheet.cx.simulate_keystrokes("h");
    draw(&mut sheet.cx);
    assert_eq!(
        shown(&sheet),
        (Some("H".into()), String::new()),
        "keys look up again"
    );
    release(&mut sheet, "h");
    sheet.cx.simulate_keystrokes("escape");
    assert!(!open(&mut sheet), "escape at rest closes the sheet");
}

#[test]
fn while_searching_keys_keep_their_search_meaning() {
    // What `r e n` and a held n leave, with and without a keyboard over the sheet.
    let after = |keyboard: bool| {
        let mut sheet = opened(2560., 1400.);
        if keyboard {
            sheet.cx.simulate_keystrokes("ctrl-f");
        } else {
            sheet
                .view
                .update(&mut sheet.cx, |view, _| view.sheet.set_keyboard(None));
            draw(&mut sheet.cx);
        }
        sheet.cx.simulate_keystrokes("r e n");
        draw(&mut sheet.cx);
        let chord = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.chord);
        repeat(&mut sheet, "n");
        let query = sheet
            .view
            .read_with(&sheet.cx, |view, _| view.sheet.query.clone());
        (chord, query)
    };
    let with = after(true);
    assert_eq!(with.0, None, "a held letter mid-search is not looked up");
    assert_eq!(with, after(false));
}

#[test]
fn a_row_under_the_pointer_lights_its_keys() {
    let mut sheet = opened(2560., 1400.);
    let label = sheet
        .cx
        .debug_bounds("sheet-label-0-0")
        .expect("the first rest row");
    sheet
        .cx
        .simulate_mouse_move(label.center(), None, gpui::Modifiers::none());
    draw(&mut sheet.cx);
    let hover = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.sheet.hover.clone());
    assert_eq!(
        hover,
        Some(super::sheet::Hover::Action("cursorDown".into()))
    );
}

#[test]
fn a_looked_up_chord_drops_the_row_lit_under_the_pointer() {
    let mut sheet = opened(2560., 1400.);
    sheet.view.update(&mut sheet.cx, |view, _| {
        view.sheet.hold(flea::keys::CONTROL);
        view.sheet.hover = Some(super::sheet::Hover::Binding {
            mask: flea::keys::CONTROL,
            cap: 0,
        });
    });
    sheet.cx.simulate_keystrokes("ctrl-c");
    draw(&mut sheet.cx);
    let (hover, chord) = sheet.view.read_with(&sheet.cx, |view, _| {
        (view.sheet.hover.clone(), view.sheet.chord)
    });
    assert!(chord.is_some(), "Ctrl + C is looked up");
    assert_eq!(hover, None, "the row lit before the lookup is gone");
}
