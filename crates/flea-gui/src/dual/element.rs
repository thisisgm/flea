//! The divider paints after both listings and keeps the underlying row from taking its press.
use crate::{
    list::Palette,
    state::Shell,
    window::{composition::Geometry, startup::View},
};
use gpui::{
    div, prelude::*, px, Context, Div, HoverListenerMode, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent,
};

impl Shell {
    pub(crate) fn dual_divider(
        &self,
        geometry: &Geometry,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        (self.view == View::Dual).then(|| {
            let hot = self.dual_drag.is_some_and(|drag| drag.live());
            // ResizeHandle.qml lights the accent while hovered or dragged.
            let lit = hot || self.dual_hover;
            let dx = self.dual_drag.map(|drag| drag.dx()).unwrap_or(0.);
            let x = geometry.panes[0].x + geometry.panes[0].width;
            let (muted, accent) = crate::dual::line_offsets(lit, dx);
            div()
                .id("dual-divider")
                .absolute()
                .left(px(x))
                .top(px(geometry.panes[0].y))
                .w(px(crate::dual::ZONE))
                .h(px(geometry.panes[0].height))
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
                .on_hover(cx.listener(|shell, hovered: &bool, _, cx| {
                    if shell.dual_hover != *hovered {
                        shell.dual_hover = *hovered;
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .absolute()
                        .left(px(muted))
                        .w(px(1.))
                        .h_full()
                        .bg(palette.muted_frame),
                )
                .children(accent.map(|offset| {
                    div()
                        .absolute()
                        .left(px(offset))
                        .w(px(1.))
                        .h_full()
                        .bg(palette.accent_frame)
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|shell, event: &MouseDownEvent, window, cx| {
                        // The zone takes the press alone: the listing under it never focuses.
                        cx.stop_propagation();
                        shell.dual_begin(
                            f32::from(event.position.x),
                            event.click_count,
                            window,
                            cx,
                        );
                    }),
                )
        })
    }

    pub(crate) fn dual_events(div: Div, cx: &mut Context<Self>) -> Div {
        div.on_mouse_move(cx.listener(|shell, event: &MouseMoveEvent, window, cx| {
            shell.dual_move(f32::from(event.position.x), window, cx);
            let pressed = event.pressed_button == Some(MouseButton::Left);
            shell.sidebar_move(f32::from(event.position.x), pressed, window, cx);
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|shell, _, window, cx| {
                shell.dual_end(true, window, cx);
                shell.sidebar_end(window, cx);
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|shell, event: &MouseUpEvent, window, cx| {
                shell.dual_move(f32::from(event.position.x), window, cx);
                shell.dual_end(true, window, cx);
                shell.sidebar_end(window, cx);
            }),
        )
    }
}
