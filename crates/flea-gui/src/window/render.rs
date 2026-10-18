use super::{composition, startup::View};
use crate::{
    feedback::spinner,
    list::{ListElement, Palette},
    state::Shell,
};
use gpui::{div, prelude::*, px, AnyElement, Context, IntoElement, MouseButton, Render, Window};
use std::rc::Rc;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let geometry = self.geometry(window);
        let viewport = self.seam.viewport();
        let height = viewport.map_or_else(
            || geometry.rows_height(self.view == View::Dual, self.slots.strip_height),
            |viewport| f32::from(viewport.height),
        );
        let palette = Palette::inherited(&window.text_style());
        let drawn = [self.mark_drawn(0), self.mark_drawn(1)];
        for (side, &drawn) in drawn.iter().enumerate() {
            let Some(host) = &self.panes[side] else {
                continue;
            };
            let mark = &host.pane.swap.mark;
            mark.rendered();
            let changed = drawn != mark.shown();
            mark.show(drawn, palette.metadata_frame);
            if changed {
                self.pane_mark_shown(side, drawn, window, cx);
            }
        }
        let mut panes: [Option<AnyElement>; 2] = [None, None];
        for (side, host) in self.panes.iter_mut().enumerate() {
            let Some(host) = host else { continue };
            if side == 1 && self.view != View::Dual {
                continue;
            }
            host.pane.set_height(height);
            self.seam.frame(&mut host.pane, &host.list.borrow(), window);
            if !host.stopped {
                if let (Some((start, count)), Some(backend)) =
                    (host.pane.wanted_window(), &host.backend)
                {
                    if let Err(error) = backend.window(start, count) {
                        host.stopped = true;
                        host.record_error(error, &self.locale);
                    }
                }
            }
            let list = ListElement::new(
                Rc::clone(&host.pane.held),
                host.pane.scroll_y,
                host.pane.cursor,
                Rc::clone(&host.list),
            );
            panes[side] = Some(
                div()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |shell, _, _, cx| shell.focus_pane(side, cx)),
                    )
                    .when(
                        host.pane.state.kind == crate::pane::states::Kind::Ready,
                        |div| div.child(list),
                    )
                    .children(self.slots.states[side].clone())
                    .when(drawn[side], |div| {
                        div.child(
                            gpui::div()
                                .absolute()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(spinner::GAP))
                                .child(spinner::Slot(Rc::clone(&host.pane.swap.mark)))
                                .child(
                                    gpui::div()
                                        .text_size(host.list.borrow().typeface.caption)
                                        .font_family(
                                            host.list.borrow().typeface.font.family.clone(),
                                        )
                                        .text_color(palette.metadata)
                                        .child(host.state_text.message.clone()),
                                ),
                        )
                    })
                    .into_any_element(),
            );
        }
        if let Some(viewport) = viewport {
            return div()
                .size_full()
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .on_scroll_wheel(cx.listener(Self::wheel))
                .child(
                    div()
                        .w(viewport.width)
                        .h(viewport.height)
                        .children(panes[0].take()),
                );
        }
        let paths = self.path_labels.clone();
        let status = self.panes[self.focus_side]
            .as_ref()
            .map(|host| host.status.clone())
            .unwrap_or_default();
        composition::render(
            &geometry,
            &self.slots,
            panes,
            paths,
            status,
            &self.labels,
            self.view == View::Dual,
        )
        .track_focus(&self.focus)
        .on_key_down(cx.listener(Self::key))
        .on_scroll_wheel(cx.listener(Self::wheel))
    }
}
