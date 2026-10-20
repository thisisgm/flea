//! The Settings card and the sheet drawn in a test window, driven by real keystrokes.
use super::seams::Request;
use super::sheet::{Sheet, Sources};
use super::sheet_view::SheetView;
use super::tests::{english, panel};
use super::view::SettingsView;
use gpui::{Focusable, TestAppContext};
use std::cell::RefCell;
use std::rc::Rc;

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus.clone()
    }
}

#[gpui::test]
fn keystrokes_reach_the_panel_and_requests_reach_the_host(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.subscribe(&view, move |_, request: &Request, _| sink.borrow_mut().push(request.clone())).detach();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("j space");
    view.update(cx, |view, _| {
        assert_eq!(view.panel.document().get("rememberViewPerFolder").and_then(flea::jsondoc::Json::as_bool), Some(true));
    });
    cx.simulate_keystrokes("/ t h u m b");
    view.update(cx, |view, _| assert_eq!(view.panel.title(), "Settings"));
    cx.simulate_keystrokes("escape escape");
    assert_eq!(*seen.borrow(), [Request::Close]);
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn every_section_draws_in_a_frame(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    for section in flea::ui_logic_st1::pages::SECTIONS.iter().map(|section| section.id).chain(["columns"]) {
        view.update(cx, |view, cx| {
            view.panel.show_section(section);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

#[gpui::test]
fn the_sheet_draws_its_groups_and_its_query(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SheetView::new(Sheet::new("default", english(), Sources::default()), cx));
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes("m u t e");
    view.update(cx, |view, _| assert_eq!(view.sheet.ranked.len(), 1));
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
