use super::{composition, startup::View};
use crate::{list::ListElement, state::Shell};
use gpui::{div, prelude::*, AnyElement, Context, IntoElement, MouseButton, Render, Window};
use std::rc::Rc;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.capture.report(window, cx);
        let geometry = self.geometry(window);
        let background = crate::list::Palette::inherited(&window.text_style()).background;
        let viewport = self.seam.viewport();
        let height = viewport.map_or_else(
            || geometry.rows_height(self.view == View::Dual, self.slots.strip_height),
            |viewport| f32::from(viewport.height),
        );
        let mut panes: [Option<AnyElement>; 2] = [None, None];
        let marks = self.visual_marks();
        for (side, host) in self.panes.iter_mut().enumerate() {
            let Some(host) = host else { continue };
            if side == 1 && self.view != View::Dual {
                continue;
            }
            host.pane.set_height(height);
            if let (Some((cursor, selection)), true) = (&marks, side == self.focus_side) {
                crate::visual::apply_marks(host, *cursor, selection);
            }
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
                    .child(list)
                    .into_any_element(),
            );
        }
        self.visual_frame(window, cx);
        if let Some(viewport) = viewport {
            return div()
                .size_full()
                .bg(background)
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
            composition::Content {
                paths,
                status,
                labels: &self.labels,
            },
            self.view == View::Dual,
            self.visual
                .pending
                .as_ref()
                .map(|_| self.visual.probe.clone()),
        )
        .bg(background)
        .track_focus(&self.focus)
        .on_key_down(cx.listener(Self::key))
        .on_scroll_wheel(cx.listener(Self::wheel))
    }
}
