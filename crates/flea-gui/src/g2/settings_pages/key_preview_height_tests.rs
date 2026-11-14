//! Your Keys uses the same fixed two-column bands and updates its extent when items change.
use super::{tests::panel, view::SettingsView};
use flea::ui_logic_settings_pages::{
    rows::{Kind, Row},
    Text,
};
use gpui::{px, size, TestAppContext};

#[gpui::test]
fn fx482_user_key_preview_scrolls_to_hint_and_reflows_after_shrinking(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    cx.simulate_scale_factor_change(1.6);
    cx.simulate_resize(size(px(650.), px(700.)));
    view.update(cx, |view, cx| {
        view.panel.show_section("keys");
        let mut user = view
            .panel
            .rows
            .iter()
            .find(|row| row.kind == Kind::KeyPreview)
            .unwrap()
            .clone();
        user.id = "userKeys".into();
        user.items = user.items.iter().cycle().take(30).cloned().collect();
        view.panel.rows.push(user);
        view.panel.rows.push(Row {
            id: "fx482-tail".into(),
            ..Row::hint(Text::data("Keyboard shortcuts."))
        });
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let tall = view.read_with(cx, |view, _| {
        view.probe.bounds("row:userKeys").unwrap().size.height
    });
    view.update(cx, |view, cx| {
        view.panel.wheel(10_000.);
        assert!(view.panel.scroll > 0.);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(cx, |view, _| {
        let hint = view.probe.bounds("row:fx482-tail").unwrap();
        let mask = view.probe.bounds("row:fx482-tail:mask").unwrap();
        assert!(hint.top() >= mask.top() && hint.bottom() <= mask.bottom() + px(2.));
    });
    view.update(cx, |view, cx| {
        view.panel
            .rows
            .iter_mut()
            .find(|row| row.id == "userKeys")
            .unwrap()
            .items
            .truncate(3);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(cx, |view, _| {
        let user = view.probe.bounds("row:userKeys").unwrap();
        let hint = view.probe.bounds("row:fx482-tail").unwrap();
        assert!(user.size.height < tall);
        assert_eq!(view.panel.scroll, 0.);
        assert!(hint.top() >= user.bottom());
    });
}
