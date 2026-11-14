//! Compose the committed track Elements and the shared empty and refusal producers inside each track's clip.
use super::Columns;
#[path = "columns_render/census.rs"]
mod census;
use render_tile::{loading_tile, tile};
#[path = "columns_render/render_tile.rs"]
mod render_tile;
#[path = "columns_render/rest.rs"]
mod rest;
use crate::{
    columns::{element::TrackElement, geometry::Slot, paint::Focus, TrackRows, TrackState},
    state::Shell,
};
use gpui::{div, prelude::*, px, AnyElement, AppContext, Context, IntoElement};
use std::rc::Rc;

impl Shell {
    pub(crate) fn render_columns(
        &mut self,
        side: usize,
        mark_drawn: bool,
        held_contents: Option<Rc<crate::g2::file_search::contents::Held>>,
        mut rows: crate::list::RowStates,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.advance_columns_scroll(side, window, cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        // A headline wider than the span from the active column to the edge elides to fit it, as hero_span places it there.
        let room = self.panes[side]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
            .map_or(0., |columns| {
                columns.area.layout.width - columns.area.layout.active_x()
            });
        let no_match = self.no_match_hero(side, room, window, cx);
        let blocked = self.input_blocked(cx) || cx.has_active_drag(); // No hover under a held drag, as List.
        let renaming = self.renaming_row(side);
        let grow = self.grows[side];
        let tag_slot = self.panes[side]
            .as_ref()
            .is_some_and(|host| self.tag_slot(side, host.pane.held.listing));
        let dropping = self.file_column_cue(side, window, cx);
        let state_view = self.pane_state_view(side, cx);
        let (menu_target, rename_slot) = self.column_menu_focus(side);
        let host = self.panes[side].as_mut()?;
        let active = host.columns.as_mut()?;
        active.settle_hover(host.pane.held.listing, blocked || host.pane.busy());
        rest::rehover(host, blocked, window);
        let active = host.columns.as_mut()?;
        rows.hovered = active
            .hover
            .as_ref()
            .filter(|hover| hover.slot == Slot::Active)
            .map(|hover| hover.index);
        let contents_scroll = active.scroll[Slot::Active.index()]
            .input
            .drawn(host.pane.scroll_y);
        let (row_height, band) = host.columns_rows();
        let peek_row = host.list.borrow().columns_row();
        let grey = !rows.focused;
        let mut blocks = Self::contents_element(
            host,
            &self.launch.settings.data,
            held_contents,
            rows,
            contents_scroll,
            window,
            cx,
        );
        let columns = host.columns.as_mut()?;
        columns.area.set_row(peek_row);
        let tracks = Rc::clone(columns.area.tracks());
        columns.painters.retain(&tracks);
        for slot in 0..columns.heroes.len() {
            if !tracks
                .iter()
                .any(|track| track.slot.index() == slot && track.state == TrackState::Loading)
            {
                columns.loading_since[slot].set(None);
            }
            if !tracks.iter().any(|track| track.slot.index() == slot) {
                if let Some(hero) = &columns.heroes[slot] {
                    hero.update(cx, |hero, cx| hero.hide(cx));
                }
                columns.shown[slot] = None;
            }
        }
        let list = host.list.borrow();
        let typeface = list.typeface.clone();
        let words = Rc::clone(&list.view.words);
        let languages = list.view.lang_marks;
        let query: Rc<str> = match host.filter.shown {
            Some(_) if !host.pane.state.search => host.filter.owner.query(),
            _ => &host.pane.state.query,
        }
        .into();
        let focus = Rc::new(Focus {
            drop: dropping,
            cursor: (!host.pane.busy()).then_some(host.pane.cursor),
            selected: host.selected.clone().into(),
            renaming,
            rename_slot,
            menu_target,
            grow,
            hover: columns
                .hover
                .as_ref()
                .map(|hover| (hover.slot, hover.index)),
            hover_wash: crate::list::hover_wash::HoverWash::on(cx),
            grey,
        });
        drop(list);
        let clip = self.clipboard.column_marks();
        let mut area = div().size_full().relative().overflow_hidden();
        let child = super::with_pane(
            &host.pane,
            &host.nav,
            host.filter.shown.as_deref(),
            |pane| pane.child(),
        );
        columns.counters.blank_frames += usize::from(tracks.iter().any(|view| {
            let requested = if view.slot == Slot::Child {
                !child.is_empty() && columns.area.drawn.contains(&child)
            } else {
                columns.area.drawn.contains(&view.path)
            };
            view.slot != Slot::Active
                && requested
                && view.state == TrackState::Rows
                && match &view.rows {
                    TrackRows::Blank => true,
                    TrackRows::Peek(rows) => rows.is_empty(),
                    _ => false,
                }
        }));
        for (at, view) in tracks.iter().enumerate() {
            // HIG cols-filter-nomatch and cols4-hero-search: nothing stands right of an active column that matched nothing.
            if no_match.is_some() && view.slot.index() > Slot::Active.index() {
                continue;
            }
            let paint = columns.painters.painter(view.slot, &typeface);
            let scroll = if view.slot == Slot::Active {
                host.pane.scroll_y
            } else {
                columns.area.scroll(view.slot)
            };
            let mut element = TrackElement::new(
                Rc::clone(&tracks),
                at,
                Some(Rc::clone(&host.pane.held)),
                Rc::clone(&focus),
                columns.scroll[view.slot.index()].input.drawn(scroll),
                clip.clone(),
                paint,
                Rc::clone(&words),
            )
            .with_languages(languages);
            element = element
                .with_tag_slot(view.slot == Slot::Active && tag_slot)
                .with_rows(peek_row, peek_row);
            if view.slot == Slot::Active {
                element = element
                    .with_rows(row_height, band)
                    .with_sizes(Rc::new(crate::columns::walks::Walked {
                        list: Rc::clone(&host.list),
                        listing: host.pane.held.listing,
                    }))
                    .with_pictures(Rc::clone(&host.list.borrow().pictures))
                    .with_filter(host.filter.shown.clone())
                    .with_location(host.pane.state.search)
                    .with_query(Rc::clone(&query));
            }
            let mut track = div()
                .absolute()
                .left(px(view.x))
                .top_0()
                .w(px(view.width))
                .h_full()
                .overflow_hidden()
                .child(element);
            if view.slot == Slot::Active {
                track = track.children(blocks.take());
            }
            if !matches!(view.rows, TrackRows::Preview) && view.slot != Slot::Active {
                track = track.child(super::scroll_bar::surface(
                    cx.entity(),
                    side,
                    view.slot,
                    Rc::clone(&tracks),
                    focus.grow_for(view.slot),
                ));
            }
            if matches!(view.rows, TrackRows::Preview) {
                if columns.preview_hosted {
                    if let Some(preview) = &host.preview {
                        track = track.child(tile().child(preview.clone()));
                    }
                }
            } else if view.slot != Slot::Active {
                sync_tile(
                    columns,
                    view,
                    self.locale.revision,
                    &self.locale.catalog,
                    cx,
                );
                let slot = view.slot.index();
                if let Some(mark) = columns.loading[slot]
                    .as_ref()
                    .filter(|_| view.state != TrackState::Loading)
                {
                    mark.show(false, palette.metadata_frame);
                }
                if view.state == TrackState::Empty {
                    track = track.child(tile().children(columns.heroes[slot].clone()));
                } else if matches!(view.state, TrackState::Locked(_)) {
                    track = track.child(tile().children(columns.notices[slot].clone()));
                } else if view.state == TrackState::Loading {
                    let mark = Rc::clone(columns.loading[slot].get_or_insert_with(Rc::default));
                    let since = Rc::clone(&columns.loading_since[slot]);
                    let shell = cx.entity().downgrade();
                    let signal = gpui::canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            if since.get().is_some()
                                || bounds.size.width <= px(0.)
                                || bounds.size.height <= px(0.)
                            {
                                return;
                            }
                            let Some(entity) = shell.upgrade() else {
                                return;
                            };
                            let pending = entity.read_with(cx, |shell, _| {
                                let host = shell.panes[side].as_ref()?;
                                since.set(Some(host.pane.now()));
                                Some(host.columns.as_ref()?.paint_pending.replace(true))
                            });
                            if pending != Some(false) {
                                return;
                            }
                            let shell = shell.clone();
                            window.defer(cx, move |window, cx| {
                                let _ =
                                    shell.update(cx, |shell, cx| shell.columns_drawn(window, cx));
                            });
                        },
                    )
                    .absolute()
                    .size_full();
                    track = track.child(
                        loading_tile(
                            mark,
                            &typeface,
                            palette.muted,
                            self.locale.catalog.tr("window.loading").into(),
                            window,
                        )
                        .child(signal),
                    );
                }
            } else if mark_drawn {
                track = track.child(loading_tile(
                    Rc::clone(&host.pane.swap.mark),
                    &typeface,
                    palette.muted,
                    host.state_text.message.clone().into(),
                    window,
                ));
            }
            if view.slot == Slot::Active {
                // The track's own bar sits above the shared gestures, so a lane press never reaches a row.
                track = track
                    .child(super::gestures::surface(side, palette, cx))
                    .child(super::scroll_bar::surface(
                        cx.entity(),
                        side,
                        view.slot,
                        Rc::clone(&tracks),
                        focus.grow_for(view.slot),
                    ));
            }
            area = area.child(track);
        }
        if let Some((hero, natural)) = no_match {
            let (x, width) = crate::pane::states::hero_span(
                columns.area.layout.width,
                columns.area.layout.active_x(),
                natural,
            );
            area = area.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top_0()
                    .w(px(width))
                    .h_full()
                    .debug_selector(|| "columns-no-match".into())
                    .child(hero),
            );
        } else if let Some(state) = state_view {
            if let Some(natural) = self.surfaces[side].natural_width(window, cx) {
                let (x, width) = crate::pane::states::hero_span(
                    columns.area.layout.width,
                    columns.area.layout.active_x(),
                    natural,
                );
                area = area.child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top_0()
                        .w(px(width))
                        .h_full()
                        .debug_selector(|| "columns-state-slot".into())
                        .child(state),
                );
            }
        }
        area = area.children(census::surface(
            host.pane.path(),
            &columns.painters,
            &tracks,
        ));
        area = area.children(super::edge::surface(self, side, cx));
        Some(
            super::gestures::draggable(area, side, cx)
                .capture_any_mouse_down(cx.listener(move |shell, event, window, cx| {
                    shell.columns_file_tap(side, event, window, cx)
                }))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |shell, event, window, cx| {
                        shell.columns_pointer(side, event, window, cx)
                    }),
                )
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |shell, event, window, cx| {
                        shell.columns_pointer(side, event, window, cx)
                    }),
                )
                .on_scroll_wheel(
                    cx.listener(move |shell, event, _, cx| shell.columns_wheel(side, event, cx)),
                )
                .into_any_element(),
        )
    }
}

fn sync_tile(
    columns: &mut Columns,
    view: &crate::columns::TrackView,
    revision: u64,
    catalog: &flea::i18n::I18n,
    cx: &mut Context<Shell>,
) {
    let at = view.slot.index();
    let key = (view.path.clone(), view.state, revision);
    if columns.shown[at].as_ref() == Some(&key) {
        return;
    }
    if let Some(hero) = &columns.heroes[at] {
        hero.update(cx, |hero, cx| hero.hide(cx));
    }
    if view.state == TrackState::Empty {
        let hero = columns.heroes[at].get_or_insert_with(|| {
            cx.new(|cx| {
                let mut hero = crate::g2::status_feedback::hero::EmptyHero::new(catalog, cx);
                hero.set_overlaid();
                hero
            })
        });
        hero.update(cx, |hero, cx| {
            hero.translated(catalog);
            hero.show(columns.area.hero_settled.as_deref() != Some(&view.path), cx);
        });
    } else if let TrackState::Locked(mode) = view.state {
        let notice = columns.notices[at].get_or_insert_with(|| {
            cx.new(|_| crate::g2::status_feedback::state::StateMessage::new())
        });
        notice.update(cx, |notice, cx| {
            notice.set(
                Some(crate::g2::status_feedback::state::Facts::failed(
                    "scan",
                    "Permission denied",
                    mode,
                    catalog,
                )),
                cx,
            )
        });
    }
    columns.shown[at] = Some(key);
}
