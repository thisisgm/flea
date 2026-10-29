//! The card's one framed button, Open With, in DialogButton's recipe.
use super::{
    look::{self, Look},
    Hit, QuickLook,
};
use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, MouseButton, Window};

/// DialogButton: a 30 px framed label, foreground frame focused, 8% wash hovered, 14% pressed with a 2% inset.
pub fn open_with(
    ql: &QuickLook,
    look: &Look,
    window: &mut Window,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let label = ql.i18n.tr("preview.open_with");
    let style = window.text_style();
    let run = style.to_run(label.len());
    let advance = window
        .text_system()
        .layout_line(&label, px(look::BODY), &[run], None)
        .width;
    let width = (advance + px(2. * look::GAP + 2.)).max(px(look::SLOT_W));
    let pressed = ql.pressed == Some(Hit::OpenWith);
    let focused = ql.focused == Some(Hit::OpenWith);
    let hovered = ql.hovered == Some(Hit::OpenWith);
    let inset_x = if pressed {
        (f32::from(width) * (1. - look::PRESS_SCALE) / 2.).round()
    } else {
        0.
    };
    let inset_y = if pressed {
        (look::BUTTON_H * (1. - look::PRESS_SCALE) / 2.).round()
    } else {
        0.
    };
    let wash = if pressed {
        Some(Look::wash(look.foreground, true))
    } else if hovered {
        Some(Look::wash(look.foreground, false))
    } else {
        None
    };
    let mut frame = div()
        .debug_selector(|| "pv1-open-with-frame".into())
        .absolute()
        .left(px(inset_x))
        .right(px(inset_x))
        .top(px(inset_y))
        .bottom(px(inset_y))
        .border_1()
        .border_color(if focused {
            look.foreground
        } else {
            look.muted_frame
        })
        .rounded(look.control_corner);
    if let Some(wash) = wash {
        frame = frame.bg(wash);
    }
    div()
        .id("pv1-open-with")
        .debug_selector(|| "pv1-open-with".into())
        .relative()
        .mt(px(look::GAP))
        .w(width)
        .h(px(look::BUTTON_H))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(frame)
        .child(super::card::text(&label, look::BODY, look.foreground))
        .on_hover(cx.listener(|ql, hovered: &bool, _, cx| {
            if !ql.is_open() {
                return;
            }
            let now = hovered.then_some(Hit::OpenWith);
            if ql.hovered != now {
                ql.hovered = now;
                cx.notify();
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|ql, _, _, cx| {
                if !ql.is_open() {
                    return;
                }
                ql.pressed = Some(Hit::OpenWith);
                cx.notify();
            }),
        )
        .on_click(cx.listener(|ql, _: &ClickEvent, _, cx| {
            ql.pressed = None;
            ql.activate(Hit::OpenWith, cx);
        }))
        .into_any_element()
}
