//! The Add Plugin source is exercised under its real Settings parent and its parent focus handle.
use super::{
    page::Card,
    page_tests::{builtins, english, listing, Recorder},
    seam::Tokens,
    view::PluginsPane,
};
use crate::g2::settings_pages::{Facts, MemoryStore, Panel, SettingsView};
use gpui::{
    AppContext, Entity, PlatformInputHandler, PlatformWindow, TestAppContext, VisualTestContext,
};
use std::rc::Rc;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn input<R>(cx: &mut VisualTestContext, f: impl FnOnce(&mut PlatformInputHandler) -> R) -> R {
    let handle = cx.update(|window, _| window.window_handle());
    let mut window = cx.cx.test_window(handle);
    let mut input = window
        .take_input_handler()
        .expect("Add Plugin registers its actual source field");
    let result = f(&mut input);
    window.set_input_handler(input);
    cx.run_until_parked();
    result
}

pub(super) fn mounted(
    cx: &mut TestAppContext,
) -> (Entity<PluginsPane>, Rc<Recorder>, &mut VisualTestContext) {
    let wire = Rc::new(Recorder::default());
    let sent = wire.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let tokens = Tokens::interim(&crate::list::Palette::inherited(
            &window.text_style(),
            crate::palette::Theme::default(),
        ));
        let plugin = cx.new(|cx| PluginsPane::new(english(), sent, tokens, 8., cx));
        plugin.update(cx, |plugin, cx| {
            plugin.receive(&listing(1, builtins("{}")), cx)
        });
        let mut panel = Panel::new(
            Box::new(MemoryStore::new(flea::jsondoc::parse("{}").unwrap())),
            Facts::default(),
            english(),
        );
        panel.show_section("plugins");
        let mut view = SettingsView::new(panel, cx);
        view.plugins = Some(plugin);
        window.focus(&view.focus, cx);
        view
    });
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    let plugin = view.read_with(cx, |view, _| view.plugins.clone().unwrap());
    (plugin, wire, cx)
}

#[gpui::test]
fn i50_settings_plugin_owns_mounted_focus(cx: &mut TestAppContext) {
    let (plugin, wire, cx) = mounted(cx);
    plugin.update_in(cx, |plugin, window, cx| {
        window.focus(plugin.focus_handle(), cx);
    });
    draw(cx);
    input(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    cx.simulate_keystrokes("enter tab down");
    plugin.read_with(cx, |plugin, _| {
        assert!(
            matches!(plugin.page.card(), Card::Add(card) if card.text.is_empty() && !card.busy)
        );
        assert_eq!(plugin.editor.entry.text, "語");
    });
    input(cx, |input| input.replace_text_in_range(None, "abc"));
    cx.simulate_keystrokes("enter");
    plugin.read_with(cx, |plugin, _| {
        assert!(matches!(plugin.page.card(), Card::Add(card) if card.text == "abc" && card.busy));
    });
    let request = flea::jsondoc::parse(&wire.last()).unwrap();
    assert_eq!(
        request.get("source").and_then(flea::jsondoc::Json::as_str),
        Some("abc")
    );
}

#[gpui::test]
fn i50_settings_plugin_source_mounted(cx: &mut TestAppContext) {
    let (plugin, wire, cx) = mounted(cx);
    input(cx, |input| {
        input.replace_and_mark_text_in_range(None, "https://example.test/語", None)
    });
    draw(cx);
    plugin.read_with(cx, |plugin, _| {
        let Card::Add(card) = plugin.page.card() else {
            panic!("Add card stays open")
        };
        assert!(card.text.is_empty());
        assert!(!card.can_add());
        assert!(plugin.editor.drawn.borrow().marked.is_some());
    });
    let before = wire.last();
    cx.simulate_keystrokes("enter tab up down");
    assert_eq!(
        wire.last(),
        before,
        "candidate keys cannot install a preedit"
    );
    input(cx, |input| {
        input.replace_text_in_range(None, "https://example.test/語")
    });
    let handle = cx.update(|window, _| window.window_handle());
    let mut retained = cx.cx.test_window(handle).take_input_handler().unwrap();
    cx.simulate_keystrokes("enter");
    let request = flea::jsondoc::parse(&wire.last()).unwrap();
    assert_eq!(
        request.get("op").and_then(flea::jsondoc::Json::as_str),
        Some("add")
    );
    assert_eq!(
        request.get("source").and_then(flea::jsondoc::Json::as_str),
        Some("https://example.test/語")
    );
    retained.replace_text_in_range(None, "late");
    assert!(retained.selected_text_range(false).is_none());
    plugin.read_with(cx, |plugin, _| {
        let Card::Add(card) = plugin.page.card() else {
            panic!("busy Add card remains")
        };
        assert_eq!(card.text, "https://example.test/語");
    });
}

#[gpui::test]
fn i50_settings_plugin_escape_precedes_close(cx: &mut TestAppContext) {
    let (plugin, wire, cx) = mounted(cx);
    input(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    cx.simulate_keystrokes("escape");
    plugin.read_with(cx, |plugin, _| {
        assert!(matches!(plugin.page.card(), Card::Add(_)))
    });
    cx.simulate_keystrokes("escape");
    plugin.read_with(cx, |plugin, _| assert_eq!(*plugin.page.card(), Card::None));
    assert_eq!(
        wire.lines.borrow().len(),
        1,
        "composition cancellation sends no operation"
    );
}

#[gpui::test]
fn i50_settings_plugin_retained_handler_cannot_write_reopened_source(cx: &mut TestAppContext) {
    let (plugin, wire, cx) = mounted(cx);
    let handle = cx.update(|window, _| window.window_handle());
    let mut retained = cx.cx.test_window(handle).take_input_handler().unwrap();
    cx.simulate_keystrokes("escape enter");
    draw(cx);
    input(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    retained.replace_text_in_range(None, "stale");
    retained.replace_and_mark_text_in_range(None, "stale", None);
    retained.unmark_text();
    retained.paste(gpui::ClipboardItem::new_string("stale".into()));
    assert!(retained.selected_text_range(false).is_none());
    plugin.read_with(cx, |plugin, _| {
        let Card::Add(card) = plugin.page.card() else {
            panic!("Add card reopened")
        };
        assert!(card.text.is_empty());
        assert_eq!(plugin.editor.entry.text, "語");
        assert!(plugin.editor.entry.marked.is_some());
    });
    input(cx, |input| {
        input.replace_text_in_range(None, "https://example.test/current")
    });
    cx.simulate_keystrokes("enter");
    let request = flea::jsondoc::parse(&wire.last()).unwrap();
    assert_eq!(
        request.get("source").and_then(flea::jsondoc::Json::as_str),
        Some("https://example.test/current")
    );
}
