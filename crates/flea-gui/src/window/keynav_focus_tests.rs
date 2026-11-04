//! Raw control keys reach the mounted focus owner and its existing foreground frame.
use super::*;
use gpui::{point, px, size, Bounds, Pixels};

fn painted_box(cx: &mut TestAppContext, handle: Handle, bounds: Bounds<Pixels>) -> bool {
    cx.update_window(handle.into(), |_, window, cx| {
        let scale = window.scale_factor();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = crate::list::Palette::inherited(&window.text_style(), theme).foreground;
        window.painted_quads().iter().any(|quad| {
            let near = |a: f32, b: Pixels| (a / scale - f32::from(b)).abs() < 0.1;
            near(quad.bounds.origin.x.0, bounds.origin.x)
                && near(quad.bounds.origin.y.0, bounds.origin.y)
                && near(quad.bounds.size.width.0, bounds.size.width)
                && near(quad.bounds.size.height.0, bounds.size.height)
                && if bounds.size.height == px(1.) {
                    quad.background == gpui::Background::from(ink)
                } else {
                    quad.border_color == ink && near(quad.border_widths.top.0, px(1.))
                }
        })
    })
    .unwrap()
}

fn chrome(cx: &mut TestAppContext, handle: Handle, glyph: &str) {
    assert_eq!(
        call(cx, handle, "focusStop"),
        Reply::value(format!("chrome:{glyph}"))
    );
    let ring = edit(cx, handle, |shell, window, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        let index = crate::path::chrome::GLYPHS
            .iter()
            .position(|name| *name == glyph)
            .unwrap();
        assert!(chrome.focus[index].is_focused(window));
        let control = chrome.frame.borrow().controls[index];
        assert!(control.visible && control.enabled && control.focused);
        let bounds = control.bounds.expect("focused control must be mounted");
        Bounds::new(
            bounds.origin + point(px(2.), px(3.)),
            size(px(20.), px(20.)),
        )
    });
    assert!(
        painted_box(cx, handle, ring),
        "{glyph} must paint its existing focus frame"
    );
}

fn strip(cx: &mut TestAppContext, handle: Handle, expected: usize) {
    assert_eq!(call(cx, handle, "focusView"), Reply::value("strip"));
    let top_edge = edit(cx, handle, |shell, _, _| {
        let frame = &shell.strip.snapshot.as_ref().unwrap().frame;
        assert_eq!(
            frame.focus,
            Some(expected),
            "the painted strip must name the focused tab"
        );
        let bounds = shell.strip.bounds.get().expect("mounted strip");
        let (slot, width) = if expected < frame.labels.len() {
            (frame.slots[expected], frame.tab_width)
        } else {
            (frame.labels.len(), crate::tabs::bar::HIT_MIN)
        };
        Bounds::new(
            bounds.origin + point(px(14. + frame.tab_width * slot as f32 + 3.), px(3.)),
            size(px(width - 6.), px(1.)),
        )
    });
    assert!(
        painted_box(cx, handle, top_edge),
        "focused tab must paint its foreground frame"
    );
}

#[gpui::test]
fn keynav_vfix_chrome_raw_navigation_skips_hidden_and_disabled_controls(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "tab");
    key(cx, handle, "tab");
    chrome(cx, handle, "arrow-up");
    for glyph in ["search", "list", "columns", "grid", "dual", "sliders"] {
        key(cx, handle, "right");
        chrome(cx, handle, glyph);
    }
    key(cx, handle, "right");
    chrome(cx, handle, "sliders");
    key(cx, handle, "left");
    chrome(cx, handle, "dual");
    key(cx, handle, "home");
    chrome(cx, handle, "arrow-up");
    key(cx, handle, "left");
    chrome(cx, handle, "arrow-up");
    key(cx, handle, "end");
    chrome(cx, handle, "sliders");
    key(cx, handle, "ctrl-left");
    chrome(cx, handle, "sliders");
    key(cx, handle, "escape");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));

    edit(cx, handle, |shell, window, cx| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .nav
            .history
            .back
            .push(crate::navigation::history::Entry::new("/previous", "", 0));
        shell.set_view(View::Grid, window, cx);
    });
    frame(cx, handle);
    key(cx, handle, "tab");
    key(cx, handle, "tab");
    chrome(cx, handle, "arrow-left");
    for glyph in [
        "arrow-up", "filter", "sort", "list", "columns", "grid", "dual", "sliders",
    ] {
        key(cx, handle, "right");
        chrome(cx, handle, glyph);
    }
}

#[gpui::test]
fn keynav_vfix_strip_entry_navigation_activation_and_blur_are_painted(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "t");
    assert_eq!(call(cx, handle, "tabCount"), Reply::value("2"));
    key(cx, handle, "tab");
    key(cx, handle, "tab");
    key(cx, handle, "tab");
    strip(cx, handle, 1);
    for (stroke, index) in [
        ("left", 0),
        ("left", 0),
        ("end", 2),
        ("right", 2),
        ("home", 0),
        ("right", 1),
    ] {
        key(cx, handle, stroke);
        strip(cx, handle, index);
    }
    key(cx, handle, "tab");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.strip.snapshot.as_ref().unwrap().frame.focus, None)
    });
    key(cx, handle, "shift-tab");
    strip(cx, handle, 2);
    key(cx, handle, "home");
    key(cx, handle, "enter");
    assert_eq!(call(cx, handle, "tabIndex"), Reply::value("0"));
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
    key(cx, handle, "shift-tab");
    strip(cx, handle, 2);
    key(cx, handle, "space");
    assert_eq!(call(cx, handle, "tabCount"), Reply::value("3"));
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
    key(cx, handle, "shift-tab");
    strip(cx, handle, 3);
    key(cx, handle, "escape");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.strip.snapshot.as_ref().unwrap().frame.focus, None)
    });
}

#[gpui::test]
fn keynav_vfix_chrome_activation_keeps_view_control_and_settings_returns_to_list(
    cx: &mut TestAppContext,
) {
    let handle = listing(cx);
    key(cx, handle, "shift-tab");
    chrome(cx, handle, "sliders");
    key(cx, handle, "left");
    key(cx, handle, "left");
    chrome(cx, handle, "grid");
    key(cx, handle, "space");
    assert_eq!(call(cx, handle, "viewMode"), Reply::value("grid"));
    chrome(cx, handle, "grid");
    key(cx, handle, "end");
    key(cx, handle, "enter");
    edit(cx, handle, |shell, _, _| assert!(shell.settings.opened));
    key(cx, handle, "escape");
    edit(cx, handle, |shell, _, _| assert!(!shell.settings.opened));
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
    key(cx, handle, "shift-tab");
    chrome(cx, handle, "sliders");
}

#[gpui::test]
fn keynav_vfix_ey143_absent_columns_paint_wraps_before_the_next_draw(cx: &mut TestAppContext) {
    let handle = listing(cx);
    edit(cx, handle, |shell, window, cx| {
        let patch = flea::jsondoc::parse(r#"{"wrapAtEnds":true}"#).unwrap();
        shell.change_settings(patch, window, cx);
        shell.set_view(View::Columns, window, cx);
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .columns_key_frame(shell.grows[0])
            .is_none());
    });
    // Deliver real keys before a Columns prepaint exists, as a burst during the view handoff does.
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("end");
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .columns_key_frame(shell.grows[0])
            .is_none());
    });
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("j");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("0"));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("k");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("7"));
    edit(cx, handle, |shell, window, cx| {
        let patch = flea::jsondoc::parse(r#"{"wrapAtEnds":false}"#).unwrap();
        shell.change_settings(patch, window, cx);
    });
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("j");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("7"));
}

#[gpui::test]
fn keynav_vfix_disabled_chrome_control_rehomes_without_activating(cx: &mut TestAppContext) {
    let handle = listing(cx);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .nav
            .history
            .back
            .push(crate::navigation::history::Entry::new("/previous", "", 0));
    });
    frame(cx, handle);
    key(cx, handle, "tab");
    key(cx, handle, "tab");
    chrome(cx, handle, "arrow-left");
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().nav.history.back.clear();
    });
    frame(cx, handle);
    key(cx, handle, "enter");
    chrome(cx, handle, "arrow-up");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), "/files");
    });
}

#[gpui::test]
fn keynav_vfix_dual_ring_enters_strip_and_escape_preserves_the_pane(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "t");
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::Dual, window, cx)
    });
    frame(cx, handle);
    for stop in ["list", "rail", "chrome", "strip"] {
        key(cx, handle, "tab");
        assert_eq!(call(cx, handle, "focusView"), Reply::value(stop));
    }
    strip(cx, handle, 1);
    key(cx, handle, "shift-tab");
    chrome(cx, handle, "sliders");
    key(cx, handle, "tab");
    strip(cx, handle, 1);
    key(cx, handle, "escape");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
    edit(cx, handle, |shell, _, _| assert_eq!(shell.focus_side, 0));
    key(cx, handle, "shift-tab");
    strip(cx, handle, 2);
}

#[gpui::test]
fn keynav_vfix_composition_owns_raw_control_navigation_and_activation(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "tab");
    key(cx, handle, "tab");
    edit(cx, handle, |shell, _, _| shell.keyboard.set_composing(true));
    for stroke in [
        "right", "end", "home", "left", "enter", "space", "escape", "tab",
    ] {
        key(cx, handle, stroke);
        chrome(cx, handle, "arrow-up");
    }
    edit(cx, handle, |shell, _, _| {
        shell.keyboard.set_composing(false)
    });
    key(cx, handle, "right");
    chrome(cx, handle, "search");
}

#[gpui::test]
fn pb_tabs_strip_stop_keeps_the_global_close_chord(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "t");
    key(cx, handle, "shift-tab");
    strip(cx, handle, 2);
    key(cx, handle, "ctrl-q");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.lifecycle.phase(),
            crate::window::lifecycle::Phase::Draining
        )
    });
}
