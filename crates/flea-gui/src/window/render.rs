use super::{composition, startup::View};
use crate::{
    feedback::spinner,
    g2::dg2::join::{flock, Seed},
    grid::pane_view::PaneInputs,
    list::{ListElement, Palette},
    palette::Theme,
    state::Shell,
};
use gpui::{div, prelude::*, px, AnyElement, Context, IntoElement, MouseButton, Render, Window};
use std::rc::Rc;

// The opt-in test probe counts entry to the actual cached pane renderer.
#[cfg(test)]
#[derive(Default)]
struct EmptyOmiShellRenders(std::cell::Cell<usize>);
#[cfg(test)]
impl gpui::Global for EmptyOmiShellRenders {}

#[cfg(test)]
impl Shell {
    pub(crate) fn observe_empty_omi_shell_renders(cx: &mut gpui::App) {
        assert!(cx.try_global::<EmptyOmiShellRenders>().is_none());
        cx.set_global(EmptyOmiShellRenders::default());
    }

    pub(crate) fn empty_omi_shell_renders(cx: &gpui::App) -> usize {
        cx.global::<EmptyOmiShellRenders>().0.get()
    }
}

impl Shell {
    pub(crate) fn sync_footer_owner(&mut self, cx: &mut Context<Self>) {
        let external_line = self
            .trash
            .as_ref()
            .filter(|host| host.side == self.focus_side && self.slots.covers[host.side].is_some())
            .map(|host| host.messages.clone());
        self.footer(cx).update(cx, |footer, cx| {
            if footer.external_line.as_ref().map(|line| line.entity_id())
                != external_line.as_ref().map(|line| line.entity_id())
            {
                footer.external_line = external_line;
                cx.notify();
            }
        });
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        super::frame_feedback::perf_tests::shell_built();
        #[cfg(test)]
        if let Some(renders) = cx.try_global::<EmptyOmiShellRenders>() {
            renders.0.set(renders.0.get() + 1);
        }
        self.sync_font_settings(cx);
        self.sync_search_text();
        self.route_rail(window, cx);
        self.sync_rail(window, cx);
        self.settle_rail(cx);
        for side in 0..2 {
            self.flush_operations(side, cx);
        }
        self.sync_clip_marks();
        self.finish_grid_view(window, cx);
        self.capture.report(window, cx);
        if self.seam.viewport().is_none() {
            self.sync_footer_owner(cx);
        }
        self.sync_footer_height(window, cx);
        self.sync_tabs(window, cx);
        let geometry = self.geometry(window);
        self.size_trash(&geometry, cx);
        self.trash_frame(window, cx);
        self.sync_chrome(window, cx);
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let background = crate::list::Palette::inherited(&window.text_style(), theme).background;
        let viewport = self.seam.viewport();
        self.sync_selection_previews(window, cx);
        self.sync_columns(window, cx);
        let view = self.shown_view();
        let heights = std::array::from_fn(|side| {
            viewport.map_or_else(
                || geometry.content_height_for(side, view, self.slots.strip_height_for(side)),
                |viewport| f32::from(viewport.height),
            )
        });
        let palette = Palette::inherited(&window.text_style(), theme);
        let drawn = [self.mark_drawn(0), self.mark_drawn(1)];
        for (side, &drawn) in drawn.iter().enumerate() {
            let Some(host) = &self.panes[side] else {
                continue;
            };
            let mark = &host.pane.swap.mark;
            mark.rendered();
            mark.show(drawn, palette.accent);
        }
        let pane_width =
            |side: usize| viewport.map_or(geometry.panes[side].width, |size| f32::from(size.width));
        if self.view == View::Grid {
            for (side, &height) in heights.iter().enumerate() {
                if side == 1 && self.view != View::Dual {
                    continue;
                }
                let Some(host) = self.panes[side].as_mut() else {
                    continue;
                };
                host.settle_grid(
                    (pane_width(side), height),
                    &self.launch.settings.data,
                    &self.locale,
                );
            }
            // A failed Grid send ends the pane before this frame builds any surface or pane element.
            self.finish_grid_failures(window, cx);
        }
        // Every frame, so a Grid pane that is hidden or covered lets its pictures go.
        self.sync_grid_pictures(window, cx);
        for side in 0..2 {
            self.sync_surfaces(side, cx);
        }
        let mut panes: [Option<AnyElement>; 2] = [None, None];
        let renaming = [0, 1].map(|side| self.renaming_row(side).is_some());
        let mut lists = self.prepare_lists(
            heights,
            viewport.is_none().then_some(geometry.chrome.height),
            window,
            cx,
        );
        for side in 0..2 {
            let search_empty = self.search_empty(side, window, cx);
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            if side == 1 && self.view != View::Dual {
                host.drawn_view = None;
                host.drawn_list = false;
                continue;
            }
            if self.slots.covers[side].is_some() {
                host.drawn_view = None;
                host.drawn_list = false;
                continue;
            }
            if let Some(empty) = search_empty {
                host.drawn_view = Some(View::List);
                host.drawn_list = false;
                panes[side] = Some(empty);
                continue;
            }
            self.seam.frame(&mut host.pane, &host.list.borrow(), window);
            if view == View::Grid {
                host.drawn_view = Some(View::Grid);
                host.drawn_list = false;
                let input = PaneInputs {
                    side,
                    size: (pane_width(side), heights[side]),
                    data: &self.launch.settings.data,
                    locale: &self.locale,
                    palette: &palette,
                    marked: drawn[side],
                    states: self.slots.states[side].clone(),
                };
                panes[side] = Some(host.grid_pane(input, cx));
                continue;
            }
            if !host.stopped {
                let wanted = match host.filter.shown.clone() {
                    Some(shown) => host.pane.wanted_filtered_window(&shown),
                    None => host.pane.wanted_window(),
                };
                if let (Some((start, count)), Some(backend)) = (wanted, &host.backend) {
                    if let Err(error) = backend.window(start, count) {
                        host.stopped = true;
                        host.record_error(error, &self.locale);
                    }
                }
            }
            if view == View::Columns {
                panes[side] = self.render_columns(side, drawn[side], window, cx);
                let host = self.panes[side].as_mut().unwrap();
                host.drawn_view = panes[side].as_ref().map(|_| View::Columns);
                host.drawn_list = false;
                continue;
            }
            host.drawn_view = Some(View::List);
            host.drawn_list = host.pane.state.kind == crate::pane::states::Kind::Ready;
            // A standing filter's query marks its run in each drawn name, as a search's does.
            let query = match host.filter.shown {
                Some(_) if !host.pane.state.search => host.filter.owner.query(),
                _ => &host.pane.state.query,
            };
            host.list.borrow_mut().view.set_query(query);
            host.list
                .borrow_mut()
                .view
                .set_searching(host.pane.state.search);
            {
                let mut list = host.list.borrow_mut();
                let switched =
                    crate::list::columns::Switched::from_settings(&self.launch.settings.data);
                list.view.switched = if host.pane.state.search {
                    switched
                        .with(crate::list::columns::Column::Mode, false)
                        .with(crate::list::columns::Column::Date, false)
                        .with(crate::list::columns::Column::Kind, false)
                        .with(crate::list::columns::Column::Size, true)
                } else {
                    switched
                };
            }
            // A notch glides and a stroke rubber-bands in the drawn rows only; the pane keeps the clamped position.
            let list = ListElement::with_states(
                Rc::clone(&host.pane.held),
                host.input.drawn(host.pane.scroll_y),
                lists.states[side].take().expect("a mounted pane"),
                Rc::clone(&host.list),
            )
            .with_filter(host.filter.shown.clone())
            .with_search_feedback(
                (host.search.owner.session.stamps.first_rows_at == 0
                    && host.search.rows_generation == host.pane.swap.session.generation)
                    .then(|| super::search::Draw {
                        side,
                        generation: host.search.rows_generation,
                        listing: host.pane.held.listing,
                        clock: host.pane.clock.clone(),
                        shell: cx.entity().downgrade(),
                    }),
            );
            let lifter = cx.entity().downgrade();
            let payload = cx.entity().downgrade();
            panes[side] = Some(
                div()
                    .id(("file-drag", side))
                    .relative()
                    .size_full()
                    .on_drag(Seed { side }, move |_, _, window, cx| {
                        let _ =
                            lifter.update(cx, |shell, cx| shell.begin_file_drag(side, window, cx));
                        flock(cx)
                    })
                    .external_drag_payload::<Seed>(move |_, window, cx| {
                        payload.upgrade()?.read(cx).file_drag_payload(window, cx)
                    })
                    .on_drop::<Seed>(cx.listener(move |shell, _: &Seed, window, cx| {
                        shell.land_own_drag(side, window, cx);
                    }))
                    .children(lists.areas[side].take())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |shell, _, window, cx| {
                            if !shell.input_blocked(cx) {
                                shell.focus_pane(side, cx);
                                window.focus(&shell.focus, cx);
                            }
                        }),
                    )
                    .when(host.drawn_list, |div| {
                        div.child(list)
                            .child(crate::input::surface(cx.entity(), side))
                    })
                    .when(host.drawn_list && renaming[side], |div| {
                        div.child(super::rename::field::FieldElement::new(cx.entity(), side))
                    })
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
        self.settle_list_sizes(window, cx);
        self.visual_frame(window, cx);
        self.sync_chrome_probe(cx);
        for (side, &drawn) in drawn.iter().enumerate() {
            self.ineligible_loading_minimum(side, drawn, window, cx);
        }
        self.remember_paint(window, cx);
        if let Some(viewport) = viewport {
            return div()
                .size_full()
                .bg(background)
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .capture_any_mouse_down(cx.listener(Self::middle_tab_pointer))
                .child(
                    div()
                        .w(viewport.width)
                        .h(viewport.height)
                        .children(panes[0].take()),
                );
        }
        self.sync_footer(geometry.status.width, cx);
        let paths = self.path_labels.clone();
        let status = self
            .trash_message()
            .or_else(|| self.settings_message(cx))
            .or_else(|| {
                self.panes[self.focus_side]
                    .as_ref()
                    .map(|host| host.status.clone())
            })
            .unwrap_or_default();
        Self::dual_events(
            composition::render(
                &geometry,
                &self.slots,
                panes,
                composition::Content {
                    paths,
                    headers: lists.headers,
                    status,
                    labels: &self.labels,
                    typeface: self.active_typeface(),
                    focus: &self.dual_focus,
                    pane_edge: self.dual_edge(&geometry, palette),
                },
                view,
                self.visual
                    .pending
                    .as_ref()
                    .map(|_| self.visual.probe.clone()),
            )
            .children(self.dual_divider(&geometry, palette, cx))
            .bg(background)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_scroll_wheel(cx.listener(Self::wheel))
            .capture_any_mouse_down(cx.listener(Self::side_button))
            .capture_any_mouse_down(cx.listener(Self::middle_tab_pointer))
            .child(Self::list_header_input(cx)),
            cx,
        )
        .children(self.settings_surface())
        .children(self.trash_menu_layer(cx))
        .children(self.network_layers(window, cx))
        .children(self.unlock_layer(cx))
    }
}
