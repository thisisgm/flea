//! One Grid pane's element: the tile Element under the pane's pointer routing and swap overlay.
use crate::{
    feedback::spinner,
    list::Palette,
    state::Shell,
    window::{locale::Snapshot, pane::PaneHost},
};
use flea::jsondoc::Json;
use gpui::{div, prelude::*, AnyElement, AnyView, Context, IntoElement, MouseButton, Window};
use std::rc::Rc;

/// What the window gives one Grid pane beyond the pane host itself.
pub(crate) struct PaneInputs<'a> {
    pub side: usize,
    pub size: (f32, f32),
    pub data: &'a Json,
    pub locale: &'a Snapshot,
    pub palette: &'a Palette,
    pub marked: bool,
    pub states: Option<AnyView>,
    /// The row whose name the rename field covers, so its tile drops its own caption.
    pub renaming: Option<usize>,
}

impl PaneHost {
    pub(crate) fn grid_pane(
        &mut self,
        input: PaneInputs,
        window: &Window,
        cx: &mut Context<Shell>,
    ) -> AnyElement {
        let side = input.side;
        self.list.borrow_mut().park();
        let content = self.render_grid(input.size, input.data, input.locale, input.renaming);
        let palette = *input.palette;
        let scroller = crate::input::grid_surface(cx.entity(), side, move |shell, bounds| {
            let host = shell.panes[side].as_mut()?;
            host.input.hit_boxes = crate::input::views::grid_hit_boxes(host, bounds);
            let grid = host.grid.as_ref()?;
            let state = grid.state.borrow();
            let layout = state.layout?;
            Some((
                crate::input::Frame {
                    bounds,
                    row: layout.cell_height,
                    lane: layout.metrics.pad_x,
                    content: layout.extent(),
                    scroll: state.scroll,
                    listing: state.frame_held.as_ref()?.listing,
                    total: layout.total,
                    columns: layout.columns,
                    cell_width: layout.cell_width,
                    cell_origin: (layout.metrics.gap, layout.metrics.gap),
                },
                palette,
            ))
        });
        div()
            .id(("pane", side))
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, _, cx| shell.focus_pane(side, cx)),
            )
            .on_hover(cx.listener(move |shell, hovered, _: &mut Window, cx| {
                if !hovered {
                    shell.grid_leave(side, cx);
                }
            }))
            .on_mouse_move(
                cx.listener(move |shell, event, _, cx| shell.grid_hover(side, event, cx)),
            )
            .on_any_mouse_down(cx.listener(move |shell, event, window, cx| {
                shell.grid_press(side, event, window, cx)
            }))
            .when(
                self.pane.state.kind == crate::pane::states::Kind::Ready,
                |div| div.child(content),
            )
            .child(scroller)
            .children(input.states)
            .when(input.marked, |div| {
                div.child(
                    gpui::div()
                        .absolute()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .child(spinner::hero(
                            Rc::clone(&self.pane.swap.mark),
                            gpui::div()
                                .text_size(self.list.borrow().typeface.caption)
                                .font_family(self.list.borrow().typeface.font.family.clone())
                                .text_color(input.palette.metadata)
                                .child(self.state_text.message.clone()),
                            self.list.borrow().typeface.caption,
                            window,
                        )),
                )
            })
            .into_any_element()
    }
}
