//! Real window input verifies offset-pane hits and leaves global F6 dispatch with the host.
use super::{
    browser::{Activated, ShareView},
    view::NetworkView,
};
use crate::{g2::file_ops::palette::CardPalette, list::Palette, palette::Theme};
use flea::i18n::{I18n, Paths};
use gpui::{div, point, prelude::*, px, Context, Entity, Render, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub(super) fn catalog() -> (super::store_tests::Root, Arc<I18n>) {
    let root = super::store_tests::Root::new();
    let omarchy = root.0.join("omarchy");
    std::fs::create_dir_all(omarchy.join("default/i18n/en")).unwrap();
    std::fs::write(
        omarchy.join("default/i18n/en/shell.json"),
        r#"{"common.cancel":"Cancel"}"#,
    )
    .unwrap();
    let catalog = Arc::new(I18n::load(Paths {
        omarchy,
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    }));
    (root, catalog)
}
pub(super) fn palette() -> CardPalette {
    CardPalette::from_list(
        &Palette::inherited(&gpui::TextStyle::default(), Theme::default()),
        8.,
    )
}
struct Offset {
    shares: Entity<ShareView>,
}

#[gpui::test]
fn uish_misc_native_protocol_click_keeps_a_visible_field_and_its_selection(
    cx: &mut gpui::TestAppContext,
) {
    use super::dialog::Focus;
    let (_root, catalog) = catalog();
    let (host, cx) = cx.add_window_view(|_, cx| GlobalKey {
        card: cx.new(|cx| NetworkView::new(catalog, palette(), cx)),
        f6: Rc::new(Cell::new(false)),
    });
    let card = cx.update(|_, cx| host.read(cx).card.clone());
    cx.update(|window, cx| {
        card.update(cx, |v, cx| {
            v.open(window, cx);
            v.model.form.path = "before/after".into();
            v.model.focus = Focus::Path;
            v.load_input();
            v.input.move_to(3);
            v.input.select_to(6);
            v.changed(cx);
        })
    });
    for (field, protocol, expected) in [
        (Focus::Path, 2, Focus::Path),
        (Focus::Domain, 1, Focus::User),
    ] {
        cx.update(|_, cx| {
            card.update(cx, |v, cx| {
                if field != Focus::Path {
                    v.model.pick(0);
                    v.model.focus = field;
                    v.load_input();
                    v.changed(cx);
                }
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let position = cx.update(|_, cx| {
            let v = card.read(cx);
            let d = v.drawn.borrow();
            let (_, r) = d
                .hits
                .iter()
                .find(|(f, _)| *f == Focus::Protocol(protocol))
                .unwrap();
            point(
                px(d.origin.0 + r.x + r.w / 2.),
                px(d.origin.1 + r.y + r.h / 2.),
            )
        });
        cx.simulate_mouse_down(position, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(position, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.update(|_, cx| {
            let v = card.read(cx);
            assert_eq!(v.model.focus, expected);
            if field == Focus::Path {
                assert_eq!(
                    v.input.selected,
                    3..6,
                    "a protocol click preserves the live selection"
                );
                assert_eq!(v.input.text, "before/after");
            }
        });
    }
}
impl Render for Offset {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().relative().child(
            div()
                .absolute()
                .left(px(200.))
                .top(px(120.))
                .w(px(300.))
                .h(px(200.))
                .child(self.shares.clone()),
        )
    }
}

#[gpui::test]
fn pl2_browser_hits_are_local_to_the_pane_and_generated_actions_keep_the_owner(
    cx: &mut gpui::TestAppContext,
) {
    let (_root, catalog) = catalog();
    let (host, cx) = cx.add_window_view(|_, cx| Offset {
        shares: cx.new(|cx| ShareView::new(catalog, palette(), cx)),
    });
    let shares = cx.update(|_, cx| host.read(cx).shares.clone());
    let results = Rc::new(RefCell::new(Vec::new()));
    let seen = results.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shares, move |_, event: &Activated, _| {
            seen.borrow_mut().push((event.0.clone(), event.2))
        })
        .detach()
    });
    cx.update(|window, cx| {
        shares.update(cx, |v, cx| {
            v.show(
                "smb://host/".into(),
                "Host".into(),
                vec!["one".into(), "two".into()],
                9,
                window,
                cx,
            )
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let position = point(px(220.), px(167.));
    cx.simulate_mouse_down(
        position,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(&*results.borrow(), &[("smb://host/two/".into(), 9)]);
    cx.update(|window, cx| {
        shares.update(cx, |v, cx| {
            v.show(
                "smb://host/".into(),
                "Host".into(),
                vec!["one".into(), "two".into()],
                9,
                window,
                cx,
            );
            assert!(v.action("cursorDown", cx));
            assert!(v.action("open", cx));
            assert!(!v.action("open", cx), "closed browser takes no input");
        })
    });
    assert_eq!(results.borrow().len(), 2);
}

struct GlobalKey {
    card: Entity<NetworkView>,
    f6: Rc<Cell<bool>>,
}
impl Render for GlobalKey {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.f6.clone();
        div()
            .id("pl2-test-global")
            .size_full()
            .child(self.card.clone())
            .on_key_down(move |event, _, _| {
                if event.keystroke.key == "f6" {
                    seen.set(true);
                }
            })
    }
}
#[gpui::test]
fn pl2_card_leaves_global_f6_available_while_it_owns_modal_focus(cx: &mut gpui::TestAppContext) {
    let (_root, catalog) = catalog();
    let seen = Rc::new(Cell::new(false));
    let (host, cx) = cx.add_window_view(|_, cx| GlobalKey {
        card: cx.new(|cx| NetworkView::new(catalog, palette(), cx)),
        f6: seen.clone(),
    });
    let card = cx.update(|_, cx| host.read(cx).card.clone());
    cx.update(|window, cx| card.update(cx, |v, cx| v.open(window, cx)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("f6");
    assert!(seen.get());
    cx.update(|_, cx| assert!(card.read(cx).model.opened));
}

#[gpui::test]
fn pl2_field_pointer_places_the_caret_and_drag_replaces_only_its_selection(
    cx: &mut gpui::TestAppContext,
) {
    let (_root, catalog) = catalog();
    let (host, cx) = cx.add_window_view(|_, cx| GlobalKey {
        card: cx.new(|cx| NetworkView::new(catalog, palette(), cx)),
        f6: Rc::new(Cell::new(false)),
    });
    let card = cx.update(|_, cx| host.read(cx).card.clone());
    cx.update(|window, cx| {
        card.update(cx, |v, cx| {
            v.open(window, cx);
            v.model.form.host = "abcd".into();
            v.load_input();
            v.changed(cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (from, to) = cx.update(|_, cx| {
        let v = card.read(cx);
        let drawn = v.drawn.borrow();
        let text = drawn
            .text
            .iter()
            .find(|t| t.field == Some(super::dialog::Focus::Host))
            .unwrap();
        let x = px(drawn.origin.0 + text.rect.x);
        let y = px(drawn.origin.1 + text.rect.y + 15.);
        // Stay inside the caret cells when global coordinates round at a fractional origin.
        let from = point(x + text.run.line().x_for_index(1) - px(0.25), y);
        let to = point(x + text.run.line().x_for_index(3) - px(0.25), y);
        assert_eq!(v.point_units(from), 1);
        assert_eq!(v.point_units(to), 3);
        (from, to)
    });
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| {
        let v = card.read(cx);
        assert_eq!(
            v.selecting,
            Some(1),
            "field mouse-down establishes the drag anchor"
        );
        assert_eq!(v.input.selected, 1..1);
    });
    cx.simulate_mouse_move(
        to,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| assert_eq!(card.read(cx).input.selected, 1..3));
    cx.simulate_keystrokes("backspace");
    cx.update(|_, cx| assert_eq!(card.read(cx).model.form.host, "ad"));
}

#[gpui::test]
fn pl2_password_ime_refuses_composition_and_ignores_closed_callbacks(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::EntityInputHandler;
    let (_root, catalog) = catalog();
    let view = cx.add_window(|_, cx| NetworkView::new(catalog, palette(), cx));
    view.update(cx, |v, window, cx| {
        v.open(window, cx);
        v.model.focus = super::dialog::Focus::Password;
        v.load_input();
        v.replace_and_mark_text_in_range(None, "before-paint", None, window, cx);
        assert!(
            v.model.password.is_empty(),
            "an unpainted session refuses platform callbacks"
        );
    })
    .unwrap();
    cx.update_window(view.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    view.update(cx, |v, window, cx| {
        assert!(!v.accepts_text_input(window, cx));
        v.replace_and_mark_text_in_range(None, "a\u{1d11e}z", Some(1..3), window, cx);
        assert!(v.model.password.is_empty() && v.password_marked.is_none());
        v.replace_text_in_range(None, "a\u{1d11e}z", window, cx);
        assert_eq!(v.password_selection, 4..4);
        assert!(v.text_for_range(0..4, &mut None, window, cx).is_none());
        v.close(cx);
        v.replace_and_mark_text_in_range(None, "closed", None, window, cx);
        v.replace_text_in_range(None, "closed", window, cx);
        assert!(v.model.password.is_empty() && !v.input.text.contains("closed"));
        assert!(v.password_marked.is_none());
    })
    .unwrap();
}
