//! The approved busy-folder recovery control is part of the waiting state alone.
use super::{
    hero_tests::{bounds, english, frame},
    state::{Facts, StateMessage},
};
use gpui::{px, size, TestAppContext};

#[gpui::test]
fn mtp_reload_waiting_draws_the_approved_button_below_the_sentence(cx: &mut TestAppContext) {
    let catalog = english();
    let handle = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        StateMessage::new()
    });
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    handle
        .update(cx, |view, _, cx| {
            view.set(Some(Facts::waiting(&catalog)), cx)
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    let parts = bounds(
        cx,
        handle,
        &["fb1-state-mark", "fb1-state-sentence", "fb1-reload"],
    );
    assert_eq!(parts[0].size.height, px(40.));
    assert_eq!(parts[1].top() - parts[0].bottom(), px(10.));
    assert_eq!(parts[2].top() - parts[1].bottom(), px(19.));
    assert_eq!(parts[2].size.height, px(30.));
    assert!((f32::from(parts[2].center().x - parts[1].center().x)).abs() <= 0.5);
}

#[gpui::test]
fn mtp_reload_keyboard_focus_paints_the_controls_foreground_frame(cx: &mut TestAppContext) {
    let catalog = english();
    let handle = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        StateMessage::new()
    });
    handle
        .update(cx, |view, _, cx| {
            view.set(Some(Facts::waiting(&catalog)), cx)
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    let button = bounds(cx, handle, &["fb1-reload"])[0];
    handle
        .update(cx, |view, window, cx| {
            let focus = view.reload_focus(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    let focused = handle
        .update(cx, |_, window, cx| {
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let ink = crate::list::Palette::inherited(&window.text_style(), theme).foreground;
            let scale = window.scale_factor();
            window.painted_quads().iter().any(|quad| {
                (quad.bounds.origin.x.0 / scale - f32::from(button.origin.x)).abs() < 0.5
                    && (quad.bounds.origin.y.0 / scale - f32::from(button.origin.y)).abs() < 0.5
                    && quad.border_color == ink
                    && quad.border_widths.top.0 > 0.
            })
        })
        .unwrap();
    assert!(
        focused,
        "keyboard focus must repaint the existing button frame in foreground"
    );
}
