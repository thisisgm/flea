//! Filtered controls retain their writer, dependency guard and visible refusal.
use super::SettingsView;
use crate::g2::st1::{seams::MemoryStore, tests::panel};
use flea::{jsondoc::Json, ui_logic_st1::rows::Role};
use gpui::{Modifiers, TestAppContext};

#[gpui::test]
fn m9_g2rows_02_filtered_choices_disabled_controls_and_refused_write(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut panel = panel("{}");
        panel.show_section("preview");
        let view = SettingsView::new(panel, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_keystrokes("/ t h u m b");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let selected = view.read_with(cx, |view, _| {
        view.probe
            .bounds("choice:preview.thumbnails:2")
            .unwrap()
            .center()
    });
    cx.simulate_click(selected, Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.panel.query.as_deref(),
            Some("thumb"),
            "the selected choice remains a no-op"
        )
    });
    let off = view.read_with(cx, |view, _| {
        view.probe
            .bounds("choice:preview.thumbnails:0")
            .unwrap()
            .center()
    });
    cx.simulate_click(off, Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("thumb"));
        assert_eq!(
            view.panel
                .document()
                .get("preview")
                .and_then(|p| p.get("thumbnails"))
                .and_then(Json::as_str),
            Some("off")
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let usb = view.read_with(cx, |view, _| {
        view.probe.bounds("row:preview.thumbUsb").unwrap().center()
    });
    let before = view.read_with(cx, |view, _| view.panel.document().clone());
    cx.simulate_click(usb, Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(
            *view.panel.document(),
            before,
            "disabled dependent never writes"
        )
    });
    let all = view.read_with(cx, |view, _| {
        view.probe
            .bounds("choice:preview.thumbnails:2")
            .unwrap()
            .center()
    });
    cx.simulate_click(all, Modifiers::none());
    view.update(cx, |view, cx| {
        let mut store = MemoryStore::new(view.panel.document().clone());
        store.refuse = true;
        view.panel.replace_store(Box::new(store));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let before = view.read_with(cx, |view, _| view.panel.document().clone());
    cx.simulate_click(usb, Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(*view.panel.document(), before);
        assert_eq!(view.panel.query.as_deref(), Some("thumb"));
        assert!(view.panel.save_error.is_some());
        let lines = view.lines();
        let footer = lines.last().unwrap();
        assert!(footer.row.footer);
        assert_eq!(footer.row.role, Role::Error);
        assert_eq!(
            footer.row.label.resolve(&view.panel.catalog),
            "That setting could not be saved."
        );
    });
    cx.simulate_keystrokes("escape / s e t t i n g");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.lines().iter().filter(|line| line.row.footer).count(),
            1,
            "matching the error itself does not duplicate its footer"
        );
    });
}
