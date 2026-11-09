//! The path field opens the rename field's text menu on a right press and keeps the keyboard and its line through it.
use crate::path::view::PathView;
use crate::window::rename::menu::text_menu_tests::{row_ids, row_point, ROWS};
use gpui::{
    div, point, prelude::*, px, size, AppContext, ClipboardItem, Context, Entity,
    EntityInputHandler, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    Window,
};

// The field mounted 280 by 26 at (80, 20) in a 700 by 360 window, room for the card below it.
struct Host(Entity<PathView>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .absolute()
                .left(px(80.))
                .top(px(20.))
                .w(px(280.))
                .h(px(26.))
                .child(self.0.clone()),
        )
    }
}

fn field(cx: &mut TestAppContext) -> (Entity<PathView>, &mut VisualTestContext) {
    let (host, cx) = cx.add_window_view(|_, cx| {
        Host(cx.new(|cx| {
            PathView::new(
                std::sync::Arc::new(crate::test_catalogs::english()),
                Box::new(crate::path::suggestions::Unranked),
                cx,
            )
        }))
    });
    cx.simulate_resize(size(px(700.), px(360.)));
    let view = host.read_with(cx, |host, _| host.0.clone());
    view.update_in(cx, |v, w, cx| v.open(w, cx));
    cx.run_until_parked();
    view.update_in(cx, |v, w, cx| {
        v.replace_text_in_range(None, "/tmp", w, cx);
    });
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>, button: MouseButton) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, button, Modifiers::none());
    cx.simulate_mouse_up(at, button, Modifiers::none());
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn a_right_press_in_the_path_field_opens_the_text_menu(cx: &mut TestAppContext) {
    let (view, cx) = field(cx);
    cx.write_to_clipboard(ClipboardItem::new_string("/menu".into()));
    let inside = point(px(200.), px(33.));
    click(cx, inside, MouseButton::Right);
    let ids = view.read_with(cx, |v, _| row_ids(&v.text_menu.menu.borrow()));
    assert_eq!(ids, ROWS);
    // Paste goes in at the caret, the card closes and the field still edits with the keyboard.
    let paste = view.read_with(cx, |v, _| {
        row_point(&v.text_menu.menu.borrow(), "edit:paste")
    });
    click(cx, paste, MouseButton::Left);
    view.read_with(cx, |v, _| {
        assert!(!v.text_menu.opened());
        assert!(v.entry.editing, "the path field stays open");
        assert_eq!(v.entry.text, "/tmp/menu");
    });
    // The card holds the keyboard: Escape closes it and leaves the field open; the next key types.
    click(cx, inside, MouseButton::Right);
    assert!(view.read_with(cx, |v, _| v.text_menu.opened()));
    cx.simulate_keystrokes("escape");
    draw(cx);
    view.read_with(cx, |v, _| {
        assert!(!v.text_menu.opened());
        assert!(v.entry.editing, "Escape closes only the card");
    });
    cx.simulate_input("a");
    draw(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.entry.text.clone()),
        "/tmp/menua",
        "the field types once the card closes"
    );
    // Select All, then Cut empties the line onto the clipboard.
    click(cx, inside, MouseButton::Right);
    let all = view.read_with(cx, |v, _| {
        row_point(&v.text_menu.menu.borrow(), "edit:selectAll")
    });
    click(cx, all, MouseButton::Left);
    click(cx, inside, MouseButton::Right);
    let cut = view.read_with(cx, |v, _| row_point(&v.text_menu.menu.borrow(), "edit:cut"));
    click(cx, cut, MouseButton::Left);
    assert_eq!(view.read_with(cx, |v, _| v.entry.text.clone()), "");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("/tmp/menua".into())
    );
}
