//! A double-click in an output field selects the word under the pointer, a third click the whole line.
use super::{
    host::OutputView,
    model::{Action, Card, Focus},
    tests::selected,
};
use flea::ui_logic_print_convert::{composition::Layout, output::Kind};
use gpui::{
    EntityInputHandler, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
};
use std::sync::Arc;

#[gpui::test]
fn a_double_click_in_the_combine_name_selects_one_word(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|w, cx| {
        let palette =
            crate::list::Palette::inherited(&w.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::file_ops::palette::CardPalette::from_list(&palette, 8.),
            cx,
        )
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(cx, |v, w, cx| {
        assert!(v.open(
            Action::Combine,
            selected(Kind::Image, &["a.pdf", "b.pdf"]),
            w,
            cx
        ));
        if let Some(Card::Combine(c)) = &mut v.model.card {
            c.settings.layout = Layout::Grid;
        }
        v.model.focus = Some(Focus::Name);
        v.flush(cx);
    });
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        })
    };
    draw(cx);
    view.update_in(cx, |v, w, cx| {
        v.editor.entry.select_all();
        v.replace_text_in_range(None, "alpha beta", w, cx);
    });
    draw(cx);
    let at = view.read_with(cx, |v, _| {
        let bounds = v.editor.drawn.borrow().bounds.expect("field is painted");
        gpui::point(bounds.left() + gpui::px(6.), bounds.center().y)
    });
    let mut words = Vec::new();
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
        words.push(view.read_with(cx, |v, _| {
            let entry = &v.editor.entry;
            entry.text[entry.selected.clone()].to_owned()
        }));
    }
    assert_eq!(
        words[1], "alpha",
        "a double-click selects the word under the pointer"
    );
    assert_eq!(
        words[2], "alpha beta",
        "a third click selects the whole line"
    );
}
