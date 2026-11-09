//! A double-click in a card field selects the word under the pointer, a third click the whole line.
use super::cancel_click::{card_open, drawn};
use super::*;
use gpui::{Modifiers, MouseButton, Pixels, Point};

fn press(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>, clicks: usize) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: clicks,
        first_mouse: false,
    });
    visual.simulate_event(gpui::MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: clicks,
    });
    frame(cx, handle);
}

fn selected(cx: &mut TestAppContext, handle: Handle) -> String {
    edit(cx, handle, |shell, _, _| {
        let entry = &shell.rename.ime.entry;
        entry.text[entry.selected.clone()].to_owned()
    })
}

#[test]
fn a_double_click_in_the_compress_name_field_selects_one_word() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests, _other) = start(&mut cx);
    receive(
        &mut cx,
        handle,
        0,
        r#"{"t":"formats","archive":["zip","tar","7z"],"convert":false,"extract":{"archive":true,"sevenZip":true,"zip":true}}"#,
    );
    choose(
        &mut cx,
        handle,
        72,
        "compress",
        "compressPassword",
        &["/files/notes.txt"],
    );
    assert!(card_open(&mut cx, handle));
    // Name holds the keys after shift-tab; replace its text with two words joined by a hyphen, a word boundary.
    keys(&mut cx, handle, "shift-tab ctrl-a");
    keys(&mut cx, handle, "x->alpha-beta");
    let text = edit(&mut cx, handle, |shell, _, _| {
        shell.rename.ime.entry.text.clone()
    });
    assert_eq!(text, "alpha-beta", "the Name field holds the typed words");
    let field = drawn(&mut cx, handle, "op2-field-0");
    // A point over the first word: a few pixels in from the field's text start.
    let at = gpui::point(field.left() + gpui::px(20.), field.center().y);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        at,
        None,
        Modifiers::none(),
    );
    press(&mut cx, handle, at, 1);
    press(&mut cx, handle, at, 2);
    assert_eq!(
        selected(&mut cx, handle),
        "alpha",
        "a double-click selects the word under the pointer, not the whole line"
    );
    press(&mut cx, handle, at, 3);
    assert_eq!(
        selected(&mut cx, handle),
        "alpha-beta",
        "a third click selects the whole line"
    );
}
