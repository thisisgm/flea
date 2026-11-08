use super::{composition, glass::producers::BodyGround, startup::View};
use crate::{
    feedback::spinner,
    g2::dg2::join::{flock, Seed},
    grid::pane_view::PaneInputs,
    list::{ListElement, Palette},
    palette::Theme,
    state::Shell,
};
use gpui::{div, prelude::*, AnyElement, Context, IntoElement, MouseButton, Render, Window};
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

#[path = "render_footer.rs"]
mod footer;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        super::frame_feedback::perf_tests::shell_built();
        #[cfg(test)]
        if let Some(renders) = cx.try_global::<EmptyOmiShellRenders>() {
            renders.0.set(renders.0.get() + 1);
        }
        super::glass::rail_overlay(
            window,
            self.launch
                .settings
                .data
                .get("places")
                .and_then(|p| p.get("autoHide"))
                .and_then(flea::jsondoc::Json::as_bool)
                == Some(true),
            cx,
        );
        self.sync_font_settings(cx);
        self.keep_midnight_wake(cx);
        self.sync_search_text();
        self.route_rail(window, cx);
        self.sync_rail(window, cx);
        self.settle_rail(window, cx);
        for side in 0..2 {
            self.flush_operations(side, cx);
            self.check_menu_zip(side);
            let shown = self.panes[side]
                .as_ref()
                .is_some_and(|host| self.tag_slot(side, host.pane.held.listing));
            if let Some(host) = &self.panes[side] {
                host.list.borrow_mut().view.tags_shown = shown;
            }
        }
        self.sync_git();
        self.sync_clip_marks();
        self.carry_sync(window, cx);
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
        self.sync_quick_look_warm(window, cx);
        self.sync_selection_previews(window, cx);
        self.sync_columns(window, cx);
        let view = self.shown_view();
        let heights = std::array::from_fn(|side| {
            viewport.map_or_else(
                || {
                    let search = match view {
                        View::Columns => self.columns_search_strip(side),
                        View::Grid => self.grid_search_strip(side),
                        View::List | View::Dual => 0.,
                    };
                    let strip = self.slots.strip_height_for(side) + search;
                    geometry.content_height_for(side, view, strip)
                },
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
        self.sync_output_thumbnails(cx);
        for side in 0..2 {
            self.sync_surfaces(side, cx);
        }
        let drop_cues = [0, 1].map(|side| self.file_grid_cue(side, window, cx));
        let mut panes: [Option<AnyElement>; 2] = [None, None];
        let renaming = [0, 1].map(|side| self.renaming_row(side));
        let flashed = [0, 1].map(|side| self.flash_covers(side));
        // Contents blocks take the block height before the frames are built, so scroll, keys and hit-test agree.
        self.sync_contents_rows();
        let mut lists = self.prepare_lists(
            heights,
            viewport.is_none().then_some(geometry.chrome.height),
            window,
            cx,
        );
        for side in 0..2 {
            let search_empty = self.search_empty(side, window, cx);
            let contents = self.contents_blocks(side).map(Rc::new);
            let state_view = self.pane_state_view(side, cx);
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            if (side == 1 && self.view != View::Dual)
                || self.slots.covers[side].is_some()
                || flashed[side]
            {
                host.drawn_view = None;
                host.drawn_list = false;
                continue;
            }
            // Columns keeps its columns and draws the headline in its own active span.
            if let Some(empty) = search_empty.filter(|_| view != View::Columns) {
                // Over Grid the headline stands alone: no List frame or Grid layout may take its pointer.
                host.drawn_view = matches!(view, View::List | View::Dual).then_some(View::List);
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
                    focused: lists.states[side].as_ref().is_some_and(|rows| rows.focused),
                    states: state_view.clone(),
                    renaming: renaming[side],
                    drop: drop_cues[side],
                };
                panes[side] = Some(super::rename::field::with_field(
                    host.grid_pane(input, window, cx),
                    renaming[side],
                    side,
                    cx,
                ));
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
                let rows = lists.states[side].take().expect("a mounted pane");
                panes[side] = self
                    .render_columns(side, drawn[side], contents, rows, window, cx)
                    .map(|pane| super::rename::field::with_field(pane, renaming[side], side, cx));
                let host = self.panes[side].as_mut().unwrap();
                host.drawn_view = panes[side].as_ref().map(|_| View::Columns);
                host.drawn_list = false;
                continue;
            }
            host.drawn_view = Some(View::List);
            host.drawn_list = host.pane.state.kind == crate::pane::states::Kind::Ready;
            Self::configure_search_rows(host, &self.launch.settings.data, self.view == View::Dual);
            // A notch glides and a stroke rubber-bands in the drawn rows only; the pane keeps the clamped position.
            let rows = lists.states[side].take().expect("a mounted pane");
            let list = ListElement::with_states(
                Rc::clone(&host.pane.held),
                host.input.drawn(host.pane.scroll_y),
                rows.clone(),
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
            let contents_scroll = host.input.drawn(host.pane.scroll_y);
            let blocks = Shell::contents_element(
                host,
                &self.launch.settings.data,
                contents,
                rows,
                contents_scroll,
                window,
                cx,
            );
            let lifter = cx.entity().downgrade();
            panes[side] = Some(
                div()
                    .id(("file-drag", side))
                    .relative()
                    .size_full()
                    // GPUI captures this source cursor for the held file drag (HIG drag-cut-move).
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_drag(Seed { side }, move |_, _, window, cx| {
                        let _ =
                            lifter.update(cx, |shell, cx| shell.begin_file_drag(side, window, cx));
                        flock(cx)
                    })
                    .map(|div| Shell::own_file_drag(div, side, cx))
                    .when(side == 0, |div| Shell::aim_rail_on_drag_move(div, cx))
                    // A pane with no rows yet draws the list ground its rows will draw.
                    .when(!host.drawn_list, |div| {
                        div.child(super::glass::Grounds::listing(theme.background))
                    })
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
                            .children(blocks)
                            .child(crate::input::surface(cx.entity(), side))
                    })
                    .when(host.drawn_list && renaming[side].is_some(), |div| {
                        div.child(super::rename::field::FieldElement::new(cx.entity(), side))
                    })
                    .children(state_view)
                    .when(drawn[side], |div| {
                        div.child(
                            gpui::div()
                                .absolute()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .child(spinner::hero(
                                    Rc::clone(&host.pane.swap.mark),
                                    gpui::div()
                                        .text_size(host.list.borrow().typeface.caption)
                                        .font_family(
                                            host.list.borrow().typeface.font.family.clone(),
                                        )
                                        .text_color(palette.muted)
                                        .child(host.state_text.message.clone()),
                                    &host.list.borrow().typeface,
                                    window,
                                )),
                        )
                    })
                    .into_any_element(),
            );
        }
        self.network_browser(&mut panes, cx);
        self.settle_list_sizes(window, cx);
        self.visual_frame(window, cx);
        self.sync_chrome_probe(cx);
        for (side, &drawn) in drawn.iter().enumerate() {
            self.ineligible_loading_minimum(side, drawn, window, cx);
        }
        self.remember_paint(window, cx);
        if let Some(viewport) = viewport {
            return Self::file_drag_events(div(), cx)
                .size_full()
                .body_ground(background, window, cx)
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key_with_preview))
                .on_modifiers_changed(cx.listener(Self::root_modifiers_changed))
                .capture_any_mouse_down(cx.listener(Self::capture_press))
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
        let body = Self::dual_events(
            composition::render(
                &geometry,
                &self.slots,
                panes,
                composition::Content {
                    paths,
                    glass: super::glass::Grounds::for_window(window, self.focus_side, cx),
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
            .children(self.sidebar_edge(&geometry, cx))
            .body_ground(background, window, cx)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_with_preview))
            .on_modifiers_changed(cx.listener(Self::root_modifiers_changed))
            .on_scroll_wheel(cx.listener(Self::wheel))
            .capture_any_mouse_down(cx.listener(Self::side_button))
            .capture_any_mouse_down(cx.listener(Self::capture_press))
            .child(Self::list_header_input(cx)),
            cx,
        );
        Self::file_drag_events(body, cx)
            .children(self.settings_surface())
            .children(self.trash_menu_layer(cx))
            .children(self.network_layers(cx))
            .children(self.carry_flock(window, cx))
            .children(self.unlock_layer(cx))
    }
}
