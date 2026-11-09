//! Save As's name field takes the pointer as the rename field does: a press places the caret, a drag selects, a double click takes the word.
use super::filename::Filename;
use super::tests::fixture::fixture;
use super::text::EntryHost;
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext,
};
use std::ops::Range;

fn down(at: Point<Pixels>, clicks: usize) -> MouseDownEvent {
    MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: clicks,
        modifiers: Modifiers::none(),
        first_mouse: false,
    }
}

fn selected(cx: &mut VisualTestContext, field: &Entity<Filename>) -> Range<usize> {
    cx.update(|_, cx| field.read(cx).entry.selected.clone())
}

fn text(cx: &mut VisualTestContext, field: &Entity<Filename>) -> String {
    cx.update(|_, cx| field.read(cx).text().to_owned())
}

fn focused(cx: &mut VisualTestContext, field: &Entity<Filename>) -> bool {
    cx.update(|window, cx| field.read(cx).focus.is_focused(window))
}

// The window x of the caret edge before byte `at`, on the line as the field last drew it.
fn x_of(cx: &mut VisualTestContext, field: &Entity<Filename>, at: usize) -> Pixels {
    cx.update(|_, cx| {
        let drawn = field.read(cx).drawn().borrow();
        let left = drawn.bounds.expect("the field drew its line").left();
        left - drawn.line.scroll + drawn.line.x_for(at)
    })
}

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn the_save_as_field_takes_a_press_a_drag_and_a_double_click(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"saved.txt"}"#,
        None,
    );
    f.cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    frame(f.cx);
    let field_box = f.cx.debug_bounds("pk1-filename");
    let field_box = field_box.expect("the Save As field");
    let field = f
        .read(|picker| picker.browse.filename.clone())
        .expect("a save has Filename");
    // The name opens with its stem selected; a press right of the text puts the caret at its end.
    assert_eq!(selected(f.cx, &field), 0..5);
    let right = point(field_box.right() - px(4.), field_box.center().y);
    f.cx.simulate_mouse_move(right, None, Modifiers::none());
    f.cx.simulate_event(down(right, 1));
    frame(f.cx);
    assert_eq!(selected(f.cx, &field), 9..9, "the press places the caret");
    assert!(focused(f.cx, &field), "the press gives the field the keys");
    // A drag past the field's left edge selects to the start; the release ends it.
    let past = point(field_box.left() - px(40.), field_box.center().y);
    f.cx.simulate_mouse_move(past, Some(MouseButton::Left), Modifiers::none());
    f.cx.simulate_mouse_up(past, MouseButton::Left, Modifiers::none());
    assert_eq!(selected(f.cx, &field), 0..9, "the drag selects the name");
    // A move with Left held, as a later drag elsewhere sends, extends nothing once released.
    f.cx.simulate_mouse_move(right, Some(MouseButton::Left), Modifiers::none());
    assert_eq!(selected(f.cx, &field), 0..9, "the release ends the drag");
    f.cx.simulate_input("renamed copy.txt");
    assert_eq!(text(f.cx, &field), "renamed copy.txt", "typing replaces");
    // A double click takes the word under the pointer; UAX 29 keeps "copy.txt" one word, as the rename field does.
    frame(f.cx);
    double_click(f.cx, &field, 2, field_box.center().y);
    assert_eq!(selected(f.cx, &field), 0..7, "double click takes the word");
    double_click(f.cx, &field, 9, field_box.center().y);
    assert_eq!(selected(f.cx, &field), 8..16, "copy.txt is one word");
}

// A double click on the glyph at byte `at`, its two presses counted as a platform counts them.
fn double_click(cx: &mut VisualTestContext, field: &Entity<Filename>, at: usize, y: Pixels) {
    let word = point(x_of(cx, field, at) + px(1.), y);
    cx.simulate_mouse_move(word, None, Modifiers::none());
    cx.simulate_event(down(word, 1));
    cx.simulate_mouse_up(word, MouseButton::Left, Modifiers::none());
    cx.simulate_event(down(word, 2));
    cx.simulate_mouse_up(word, MouseButton::Left, Modifiers::none());
}
