//! The keyboard's state as keys come and go: scrolling held lists, rebuilt keyboards, repeats and a lost window.
use super::scroll_tests::draw;
use super::sheet_keyboard_mounted_tests::{open, opened, repeat, Open};
use gpui::{KeyDownEvent, Keystroke};

#[test]
fn a_held_list_taller_than_its_room_scrolls_and_starts_at_its_top_again() {
    let mut sheet = opened(2560., 1400.);
    sheet.view.update(&mut sheet.cx, |view, _| {
        view.sheet.hold(flea::keys::CONTROL)
    });
    draw(&mut sheet.cx);
    draw(&mut sheet.cx);
    let body = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.probe.bounds("body"))
        .expect("the held list is drawn");
    super::scroll_tests::notch(&mut sheet.cx, body.center(), 2.);
    draw(&mut sheet.cx);
    let scrolled = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.scroll);
    assert!(scrolled > 0., "Ctrl's long list scrolls under the wheel");
    sheet.view.update(&mut sheet.cx, |view, _| {
        view.sheet.hold(flea::keys::CONTROL | flea::keys::SHIFT)
    });
    let scroll = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.scroll);
    assert_eq!(scroll, 0., "another held set starts at its top");
}

#[test]
fn a_shifted_lookup_stays_while_shift_is_held_and_a_released_modifier_ends_a_chord() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("shift-g");
    draw(&mut sheet.cx);
    let chord = sheet.view.read_with(&sheet.cx, |view, _| {
        (view.sheet.chord.is_some(), view.sheet.pressed.clone())
    });
    assert_eq!(
        chord,
        (true, None),
        "Shift + G stays looked up, as Ctrl + C does"
    );
    sheet.cx.simulate_keystrokes("ctrl-c");
    let ended = sheet.view.update(&mut sheet.cx, |view, _| {
        view.sheet.hold(flea::keys::CONTROL | flea::keys::SHIFT);
        let held = view.sheet.chord.is_some();
        view.sheet.hold(flea::keys::SHIFT);
        (held, view.sheet.chord.is_none())
    });
    assert_eq!(ended, (true, true), "letting go of Ctrl ends CTRL + C");
}

#[test]
fn a_rebuilt_keyboard_forgets_the_hovered_binding_and_ranks_its_search_again() {
    let mut sheet = opened(2560., 1400.);
    let plugin = flea::ui_logic_st1::sheet_keyboard::Bound {
        cap: 0,
        mask: flea::keys::CONTROL,
        action: "je93.checksums:sha256".into(),
        label: "Verify checksum".into(),
    };
    let with = |view: &mut super::sheet_view::SheetView, add: bool| {
        let mut keyboard = view.sheet.keyboard.clone().expect("a keyboard");
        keyboard.bound.retain(|bound| bound.action != plugin.action);
        if add {
            let cap = keyboard.cap_of("j", Some("j")).expect("a J cap");
            keyboard
                .bound
                .push(flea::ui_logic_st1::sheet_keyboard::Bound {
                    cap,
                    ..plugin.clone()
                });
        }
        view.sheet.set_keyboard(Some(keyboard));
    };
    sheet.view.update(&mut sheet.cx, |view, _| with(view, true));
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet.cx.simulate_keystrokes("v e r i f");
    draw(&mut sheet.cx);
    let found = |sheet: &mut Open| {
        sheet
            .view
            .read_with(&sheet.cx, |view, _| view.sheet.ranked.len())
    };
    assert_eq!(found(&mut sheet), 1, "the plugin key is found");
    let hover = sheet.view.update(&mut sheet.cx, |view, _| {
        view.sheet.hover = Some(super::sheet::Hover::Binding { mask: 0, cap: 9999 });
        with(view, false);
        view.sheet.hover.clone()
    });
    assert_eq!(hover, None, "a hovered cap of the old board is gone");
    assert_eq!(
        found(&mut sheet),
        0,
        "the search ranks the rebuilt bindings"
    );
    draw(&mut sheet.cx);
}

#[test]
fn a_rebuilt_keyboard_keeps_the_picked_result_picked() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet.cx.simulate_keystrokes("down down");
    draw(&mut sheet.cx);
    let picked = |sheet: &mut Open| {
        sheet.view.read_with(&sheet.cx, |view, _| {
            (view.sheet.cursor, view.sheet.chosen())
        })
    };
    let before = picked(&mut sheet);
    assert!(before.1, "the arrows picked a row");
    sheet.view.update(&mut sheet.cx, |view, _| {
        let keyboard = view.sheet.keyboard.clone();
        view.sheet.set_keyboard(keyboard);
    });
    assert_eq!(
        picked(&mut sheet),
        before,
        "a refresh keeps the picked row lit"
    );
}

#[test]
fn leaving_the_search_drops_an_input_methods_uncommitted_text() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    draw(&mut sheet.cx);
    sheet.view.update(&mut sheet.cx, |view, _| {
        view.editor.entry.text = "か".into();
        view.editor.entry.marked = Some(0.."か".len());
    });
    sheet.cx.simulate_keystrokes("escape");
    draw(&mut sheet.cx);
    let state = sheet.view.read_with(&sheet.cx, |view, _| {
        (
            view.sheet.typing || !view.sheet.query.is_empty() || !view.editor.entry.text.is_empty(),
            view.editor.entry.text.clone(),
            view.editor.entry.marked.clone(),
        )
    });
    assert_eq!(
        state,
        (false, String::new(), None),
        "Escape leaves a clean field"
    );
    assert!(open(&mut sheet), "the first Escape only leaves the search");
}

#[test]
fn holding_ctrl_f_opens_the_search_once() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    let mut keystroke = Keystroke::parse("ctrl-f").expect("a key");
    keystroke.key_char = None;
    for _ in 0..3 {
        sheet.cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: true,
            prefer_character_input: false,
        });
    }
    draw(&mut sheet.cx);
    let typing = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.typing);
    assert!(typing, "key repeat leaves the search open");
}

#[test]
fn the_pointer_picks_the_first_result_of_an_open_search() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    draw(&mut sheet.cx);
    let body = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.probe.bounds("body"))
        .expect("the results are drawn");
    let first = gpui::point(
        body.left() + gpui::px(120.),
        body.top() + gpui::px(super::sheet_view::HEAD_SLOT + 12.),
    );
    sheet
        .cx
        .simulate_mouse_move(first, None, gpui::Modifiers::none());
    draw(&mut sheet.cx);
    let picked = sheet.view.read_with(&sheet.cx, |view, _| {
        (view.sheet.cursor, view.sheet.chosen())
    });
    assert_eq!(
        picked,
        (0, true),
        "the first row lights though the cursor already sat on it"
    );
}

#[test]
fn holding_escape_leaves_the_search_but_keeps_the_sheet_open() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet.cx.simulate_keystrokes("escape");
    let keystroke = Keystroke::parse("escape").expect("a key");
    for _ in 0..3 {
        sheet.cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: true,
            prefer_character_input: false,
        });
    }
    draw(&mut sheet.cx);
    let typing = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.typing);
    assert!(!typing, "the first Escape left the search");
    assert!(open(&mut sheet), "its repeats leave the sheet open");
}

#[test]
fn losing_the_window_forgets_a_held_key() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.update(|window, _| window.activate_window());
    sheet.cx.run_until_parked();
    repeat(&mut sheet, "h");
    let before = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.sheet.pressed.clone());
    assert_eq!(before.as_deref(), Some("h"), "holding H looks it up");
    sheet.cx.deactivate_window();
    draw(&mut sheet.cx);
    let after = sheet.view.read_with(&sheet.cx, |view, _| {
        (
            view.sheet.pressed.clone(),
            view.sheet.chord,
            view.sheet.held,
        )
    });
    assert_eq!(
        after,
        (None, None, 0),
        "a key let go in another window is not left held"
    );
}
