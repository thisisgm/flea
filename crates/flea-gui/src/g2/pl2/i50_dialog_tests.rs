//! All seven network fields own platform sessions; PORT and PASSWORD keep the input method off, as v0.3.12 does.
use super::{dialog::Focus, view::NetworkView};
use flea::ui_logic_pl2::form::Protocol;
use gpui::{EntityInputHandler, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn i50_dialog_field_composition_matrix_network(cx: &mut TestAppContext) {
    for focus in [
        Focus::Label,
        Focus::Host,
        Focus::Port,
        Focus::Path,
        Focus::Domain,
        Focus::User,
        Focus::Password,
    ] {
        let (view, cx) = cx.add_window_view(|w, cx| {
            let p =
                crate::list::Palette::inherited(&w.text_style(), crate::palette::Theme::default());
            NetworkView::new(
                Arc::new(crate::test_catalogs::english()),
                crate::g2::op1::palette::CardPalette::from_list(&p, 8.),
                cx,
            )
        });
        cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
        view.update_in(cx, |v, w, cx| {
            v.open(w, cx);
            v.model.form.protocol = Protocol::Smb;
            v.model.focus = focus;
            v.load_input();
            v.input.select_all();
            v.changed(cx);
        });
        cx.run_until_parked();
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        let composes = view.update_in(cx, |v, w, cx| {
            let composes = !matches!(focus, Focus::Port | Focus::Password);
            assert_eq!(v.accepts_text_input(w, cx), composes);
            v.replace_and_mark_text_in_range(None, "\u{1d11e}猫", Some(2..3), w, cx);
            if !composes {
                // No preedit, nothing stored: the secret and the number stay as they were.
                assert_eq!(v.marked_text_range(w, cx), None);
                assert!(v.model.password.is_empty() && v.model.form.port == "445");
                v.replace_text_in_range(None, "日", w, cx);
                assert_eq!(v.model.password.is_empty(), focus == Focus::Port);
                assert_eq!(
                    v.model.form.port, "445",
                    "a refused commit keeps the number"
                );
                v.replace_text_in_range(None, "8080", w, cx);
                if focus == Focus::Port {
                    assert_eq!(v.model.form.port, "8080");
                } else {
                    assert!(v.model.password.expose() == "日8080".as_bytes());
                    assert!(!v.input.text.contains('日') && !v.input.text.contains('8'));
                    let mut adjusted = Some(6..9);
                    assert!(v.text_for_range(0..3, &mut adjusted, w, cx).is_none());
                    assert_eq!(adjusted, Some(6..9));
                }
                return false;
            }
            assert_eq!(v.marked_text_range(w, cx), Some(0..3));
            assert_eq!(v.selected_text_range(false, w, cx).unwrap().range, 2..3);
            true
        });
        if !composes {
            continue;
        }
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        view.update_in(cx, |v, w, cx| {
            let bounds = gpui::Bounds::new(
                gpui::point(gpui::px(0.), gpui::px(0.)),
                gpui::size(gpui::px(1.), gpui::px(1.)),
            );
            let actual = v.bounds_for_range(0..3, bounds, w, cx).unwrap();
            assert!(actual.size.width > gpui::px(0.) && actual.size.height == gpui::px(30.));
        });
        cx.simulate_keystrokes("enter");
        assert!(view.read_with(cx, |v, _| v.model.opened
            && !v.model.busy()
            && !v.model.committed));
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |v, _| v.model.opened && v.input.marked.is_none()));
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("a b");
        assert_eq!(
            view.read_with(cx, |v, _| v.input.ime_selection().range),
            3..3
        );
        view.update_in(cx, |v, w, cx| {
            v.replace_text_in_range(None, "日", w, cx);
            v.model.focus = if focus == Focus::Host {
                Focus::Path
            } else {
                Focus::Host
            };
            v.load_input();
            v.replace_text_in_range(None, "late", w, cx);
            assert!(!v.input.text.contains("late"));
            v.close(cx);
            v.replace_and_mark_text_in_range(None, "late", None, w, cx);
            assert!(v.selected_text_range(false, w, cx).is_none());
        });
    }
}

#[gpui::test]
fn ime_label_hint_names_the_share_from_committed_text(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|w, cx| {
        let p = crate::list::Palette::inherited(&w.text_style(), crate::palette::Theme::default());
        NetworkView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::op1::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(cx, |v, w, cx| {
        v.open(w, cx);
        v.model.form.protocol = Protocol::Smb;
        v.model.focus = Focus::Port;
        v.load_input();
        assert!(!v.accepts_text_input(w, cx));
        v.model.focus = Focus::Host;
        v.load_input();
        assert!(
            v.accepts_text_input(w, cx),
            "leaving PORT turns the input method back on"
        );
        v.changed(cx);
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    view.update_in(cx, |v, w, cx| {
        v.replace_text_in_range(None, "nas", w, cx);
        v.replace_and_mark_text_in_range(None, "にほ", None, w, cx);
        assert_eq!(
            v.model.form.host, "nasにほ",
            "HOST still paints its preedit"
        );
        assert_eq!(v.derived_name(), "nas", "LABEL never mirrors a preedit");
        v.replace_text_in_range(None, "日本", w, cx);
        assert_eq!(v.derived_name(), "nas日本");
    });
}
