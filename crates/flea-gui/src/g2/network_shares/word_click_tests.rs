//! A double-click in an Add a Network Share field selects the word under the pointer, a third click the whole line.
use super::{
    dialog::Focus,
    input_tests::{catalog, palette},
    view::NetworkView,
};
use gpui::{div, point, prelude::*, px, Context, Entity, Modifiers, MouseButton, Window};

struct Host {
    card: Entity<NetworkView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.card.clone())
    }
}

#[gpui::test]
fn a_double_click_in_the_share_path_selects_one_word(cx: &mut gpui::TestAppContext) {
    let (_root, catalog) = catalog();
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        card: cx.new(|cx| NetworkView::new(catalog, palette(), cx)),
    });
    let card = cx.update(|_, cx| host.read(cx).card.clone());
    cx.update(|window, cx| {
        card.update(cx, |v, cx| {
            v.open(window, cx);
            v.model.form.path = "alpha beta".into();
            v.model.focus = Focus::Path;
            v.load_input();
            v.changed(cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let at = cx.update(|_, cx| {
        let v = card.read(cx);
        let d = v.drawn.borrow();
        let text = d
            .text
            .iter()
            .find(|t| t.field == Some(Focus::Path))
            .unwrap();
        let (_, hit) = d.hits.iter().find(|(f, _)| *f == Focus::Path).unwrap();
        point(
            v.input_origin.get().x + px(d.origin.0 + text.rect.x + 6.),
            px(d.origin.1 + hit.y + hit.h / 2.),
        )
    });
    let mut selected = Vec::new();
    for clicks in 1..=3 {
        cx.simulate_event(gpui::MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: clicks,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: clicks,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        selected.push(cx.update(|_, cx| {
            let input = &card.read(cx).input;
            input.text[input.selected.clone()].to_owned()
        }));
    }
    assert_eq!(
        selected[1], "alpha",
        "a double-click selects the word under the pointer"
    );
    assert_eq!(
        selected[2], "alpha beta",
        "a third click selects the whole line"
    );
}
