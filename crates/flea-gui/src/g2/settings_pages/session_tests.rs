//! Retained platform callbacks cannot reach a replacement Settings text session.
use super::{
    ime_tests::{draw, platform, settings},
    tests::english,
    Sheet, SheetView, Sources,
};
use gpui::{PlatformInputHandler, PlatformWindow, TestAppContext};

fn rejects_retained(input: &mut PlatformInputHandler) {
    let mut adjusted = None;
    assert!(input.selected_text_range(false).is_none());
    assert!(input.marked_text_range().is_none());
    assert!(input.text_for_range(0..1, &mut adjusted).is_none());
    assert!(adjusted.is_none());
    assert!(input.bounds_for_range(0..0).is_none());
    assert!(input.element_bounds().is_none());
    assert!(input
        .character_index_for_point(gpui::point(gpui::px(0.), gpui::px(0.)))
        .is_none());
    input.replace_text_in_range(None, "stale");
    input.replace_and_mark_text_in_range(None, "stale", None);
    input.unmark_text();
    input.paste(gpui::ClipboardItem::new_string("stale".into()));
    input.set_selected_text_range(gpui::UTF16Selection {
        range: 0..0,
        reversed: false,
    });
}

#[gpui::test]
fn i50_settings_clear_retires_all_previous_callbacks(cx: &mut TestAppContext) {
    let (view, cx) = settings(cx);
    platform(cx, |input| input.replace_text_in_range(None, "thumb"));
    draw(cx);
    let handle = cx.update(|window, _| window.window_handle());
    let mut retained = cx.cx.test_window(handle).take_input_handler().unwrap();
    let clear = view.read_with(cx, |view, _| {
        view.probe.bounds("search-clear").unwrap().center()
    });
    cx.simulate_click(clear, gpui::Modifiers::none());
    draw(cx);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    let selection = view.read_with(cx, |view, _| view.editor.entry.selected.clone());
    assert_ne!(selection, 0..0, "stale selection reset must be observable");
    rejects_retained(&mut retained);
    view.read_with(cx, |view, _| {
        assert_eq!(view.editor.entry.selected, selection);
        assert_eq!(view.panel.query.as_deref(), Some(""));
        assert_eq!(view.editor.entry.text, "語");
        assert!(view.editor.entry.marked.is_some());
    });
    platform(cx, |input| input.replace_text_in_range(None, "current"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.query.as_deref(), Some("current"))
    });
}

#[gpui::test]
fn i50_settings_sheet_clear_and_close_retire_callbacks(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SheetView::new(Sheet::new("default", english(), Sources::default()), cx);
        window.focus(&view.focus, cx);
        view
    });
    draw(cx);
    platform(cx, |input| input.replace_text_in_range(None, "mute"));
    draw(cx);
    let handle = cx.update(|window, _| window.window_handle());
    let mut retained = cx.cx.test_window(handle).take_input_handler().unwrap();
    let clear = cx.debug_bounds("sheet-query-clear").unwrap().center();
    cx.simulate_click(clear, gpui::Modifiers::none());
    draw(cx);
    rejects_retained(&mut retained);
    platform(cx, |input| {
        input.replace_and_mark_text_in_range(None, "語", None)
    });
    let selection = view.read_with(cx, |view, _| view.editor.entry.selected.clone());
    assert_ne!(selection, 0..0, "stale selection reset must be observable");
    rejects_retained(&mut retained);
    view.read_with(cx, |view, _| {
        assert_eq!(view.editor.entry.selected, selection);
    });
    platform(cx, |input| {
        assert_eq!(input.marked_text_range(), Some(0..1))
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    let mut current = cx.cx.test_window(handle).take_input_handler().unwrap();
    cx.simulate_keystrokes("escape");
    rejects_retained(&mut current);
    view.read_with(cx, |view, _| {
        assert!(view.editor.retired);
        assert!(view.editor.entry.session().is_none());
        assert!(view.sheet.query.is_empty());
    });
}
