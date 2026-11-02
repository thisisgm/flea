//! The five mounted output fields exercise the platform handler and retain numeric and operation guards.
use super::{
    host::OutputView,
    model::{Action, Card, Focus},
    tests::selected,
};
use flea::ui_logic_op4::{composition::Layout, output::Kind};
use gpui::{EntityInputHandler, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn i50_dialog_field_composition_matrix_output(cx: &mut TestAppContext) {
    for (action, kind, focus) in [
        (Action::Print, Kind::Pdf, Focus::Copies),
        (Action::Print, Kind::Pdf, Focus::Pages),
        (Action::Convert, Kind::Pdf, Focus::Pages),
        (Action::Combine, Kind::Image, Focus::Name),
        (Action::Combine, Kind::Image, Focus::Columns),
    ] {
        let (view, cx) = cx.add_window_view(|w, cx| {
            let palette =
                crate::list::Palette::inherited(&w.text_style(), crate::palette::Theme::default());
            OutputView::new(
                Arc::new(crate::test_catalogs::english()),
                crate::g2::op1::palette::CardPalette::from_list(&palette, 8.),
                cx,
            )
        });
        cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
        view.update_in(cx,|v,w,cx| {
            assert!(v.open(action,selected(kind, if action == Action::Convert { &["a.pdf"] } else { &["a.pdf", "b.pdf"] }),w,cx));
            if let Some(Card::Print(p))=&v.model.card {
                let id=p.pending.unwrap().request;
                v.line(&format!(r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Office","default":true}}],"printer":"Office","duplex":false,"twoSided":false,"color":false,"grayscale":false,"setupArgv":[]}}"#),cx);
            }
            if let Some(Card::Combine(c))=&mut v.model.card { c.settings.layout=Layout::Grid; }
            v.model.focus=Some(focus); v.flush(cx);
        });
        cx.run_until_parked();
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        // COPIES and COLUMNS are numbers: the input method stays off and only digits enter.
        let composes = !matches!(focus, Focus::Copies | Focus::Columns);
        view.update_in(cx, |v, w, cx| {
            assert_eq!(v.accepts_text_input(w, cx), composes);
            let before = v.editor.entry.text.clone();
            v.replace_and_mark_text_in_range(None, "\u{1d11e}猫", Some(2..3), w, cx);
            if composes {
                assert_eq!(v.marked_text_range(w, cx), Some(0..3));
                assert_eq!(v.selected_text_range(false, w, cx).unwrap().range, 2..3);
            } else {
                assert_eq!(v.marked_text_range(w, cx), None);
                v.replace_text_in_range(None, "日", w, cx);
                assert!(!before.is_empty() && v.editor.entry.text == before);
            }
        });
        if composes {
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
            view.update_in(cx, |v, w, cx| {
                let bounds = v.editor.drawn.borrow().bounds.expect("field is painted");
                assert!(v.bounds_for_range(0..3, bounds, w, cx).unwrap().size.width > gpui::px(0.));
                assert!(v
                    .editor
                    .drawn
                    .borrow()
                    .paint
                    .as_ref()
                    .unwrap()
                    .marked
                    .is_some());
            });
            cx.simulate_keystrokes("enter");
            assert!(view.read_with(cx, |v, _| !v.model.busy && v.model.run.is_none()));
        }
        view.update_in(cx, |v, w, cx| {
            v.replace_text_in_range(None, "12", w, cx);
            assert_eq!(v.marked_text_range(w, cx), None);
        });
        assert_eq!(view.read_with(cx, |v, _| v.editor.entry.text.clone()), "12");
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("34");
        assert_eq!(view.read_with(cx, |v, _| v.editor.entry.text.clone()), "34");
    }
}

#[gpui::test]
fn i50_dialog_closed_callback_refused_output(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|w, cx| {
        let p = crate::list::Palette::inherited(&w.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::op1::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    view.update_in(cx, |v, w, cx| {
        v.open(
            Action::Combine,
            selected(Kind::Image, &["a.png", "b.png"]),
            w,
            cx,
        );
        v.model.focus = Some(Focus::Name);
        v.flush(cx);
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    view.update_in(cx, |v, w, cx| {
        v.model.close();
        v.flush(cx);
        v.replace_text_in_range(None, "late", w, cx);
        assert!(v.selected_text_range(false, w, cx).is_none());
        v.open(
            Action::Combine,
            selected(Kind::Image, &["a.png", "b.png"]),
            w,
            cx,
        );
        v.model.focus = Some(Focus::Name);
        v.flush(cx);
        v.replace_and_mark_text_in_range(None, "late", None, w, cx);
        assert!(v.editor.entry.marked.is_none());
        assert!(!v.editor.entry.text.contains("late"));
    });
}
