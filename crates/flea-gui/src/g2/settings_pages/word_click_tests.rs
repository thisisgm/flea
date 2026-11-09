//! A double-click in the Settings text input selects the word under the pointer, a third click the whole line.
use super::ime_tests::{draw, settings};
use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

#[gpui::test]
fn a_double_click_in_the_settings_query_selects_one_word(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    cx.simulate_keystrokes("a l p h a space b e t a");
    draw(cx);
    let (text, at) = view.read_with(cx, |view, _| {
        let drawn = view.editor.drawn.borrow();
        let field = drawn.bounds.expect("the query field drew");
        let at = gpui::point(field.left() + gpui::px(8.), field.center().y);
        (view.editor.entry.text.clone(), at)
    });
    assert_eq!(text, "alpha beta");
    let selected = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let entry = &view.editor.entry;
            entry.text[entry.selected.clone()].to_owned()
        })
    };
    for clicks in 1..=3 {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: clicks,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: clicks,
        });
        draw(cx);
        let expected = ["", "alpha", "alpha beta"][clicks - 1];
        assert_eq!(selected(cx), expected, "after {clicks} clicks");
    }
}
