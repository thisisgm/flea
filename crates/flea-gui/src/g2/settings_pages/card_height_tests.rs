//! S2-12: the card is as tall as View drawn at the card's width, wrapped hints and labels included, from every section.
use super::tests::panel;
use super::view::SettingsView;
use gpui::{px, size, TestAppContext};

#[gpui::test]
fn s2_12_the_card_holds_view_as_drawn_where_its_hints_wrap(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    let mut wrapped = false;
    for width in (520..=1100).step_by(20) {
        // Tall enough that the window never caps the card.
        cx.simulate_resize(size(px(width as f32), px(1600.)));
        view.update(cx, |view, cx| {
            view.panel.show_section("view");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (drawn, card, lines) = view.read_with(cx, |view, _| {
            let panel = &view.panel;
            (
                panel.heights().iter().sum::<f32>(),
                panel.view_height,
                panel.lines.clone(),
            )
        });
        wrapped |= lines.iter().any(|&count| count > 1);
        assert!(
            (drawn - card).abs() < 0.5,
            "at {width}px View draws {drawn}px but the card was sized for {card}px"
        );
        view.update(cx, |view, cx| {
            view.panel.show_section("display");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let elsewhere = view.read_with(cx, |view, _| view.panel.view_height);
        assert!(
            (elsewhere - card).abs() < 0.5,
            "at {width}px the card is {elsewhere}px on Display and {card}px on View"
        );
    }
    assert!(
        wrapped,
        "no width wrapped a View line, so this test proves nothing"
    );
}
