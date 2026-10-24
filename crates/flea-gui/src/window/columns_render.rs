//! Compose the committed track Elements and the shared empty and refusal producers inside each track's clip.
use super::Columns;
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
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let host = self.panes[side].as_mut()?;
        let columns = host.columns.as_mut()?;
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
        let focus = Rc::new(Focus {
            cursor: (!host.pane.busy()).then_some(host.pane.cursor),
            selected: host.selected.clone().into(),
            ..Focus::default()
        });
        drop(list);
        let mut area = div().size_full().relative().overflow_hidden();
        let child = super::with_pane(&host.pane, &host.nav, |pane| pane.child());
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
            let paint = columns.painters.painter(view.slot, &typeface);
            let scroll = if view.slot == Slot::Active {
                host.pane.scroll_y
            } else {
                columns.area.scroll(view.slot)
            };
            let element = TrackElement::new(
                Rc::clone(&tracks),
                at,
                Some(Rc::clone(&host.pane.held)),
                Rc::clone(&focus),
                scroll,
                None,
                paint,
                Rc::clone(&words),
            );
            let mut track = div()
                .absolute()
                .left(px(view.x))
                .top_0()
                .w(px(view.width))
                .h_full()
                .overflow_hidden()
                .child(element);
            if view.slot != Slot::Active {
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
                            palette.metadata,
                            self.locale.catalog.tr("window.loading").into(),
                        )
                        .child(signal),
                    );
                }
            } else if mark_drawn {
                track = track.child(loading_tile(
                    Rc::clone(&host.pane.swap.mark),
                    &typeface,
                    palette.metadata,
                    host.state_text.message.clone().into(),
                ));
            }
            area = area.child(track);
        }
        if let Some(state) = self.slots.states[side].clone() {
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
        Some(
            area.on_mouse_down(
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

    pub(crate) fn column_loading_marks(
        &self,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> [Option<Rc<crate::feedback::spinner::Crawl>>; 5] {
        let mut marks = std::array::from_fn(|_| None);
        if self.shown_view() != super::super::startup::View::Columns
            || self.slots.covers[0].is_some()
        {
            return marks;
        }
        let Some(columns) = self.panes[0]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
        else {
            return marks;
        };
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        for track in columns
            .area
            .tracks()
            .iter()
            .filter(|track| track.state == TrackState::Loading)
        {
            if let Some(mark) = &columns.loading[track.slot.index()] {
                mark.show(true, palette.accent_frame);
                marks[track.slot.index()] = Some(Rc::clone(mark));
            }
        }
        marks
    }

    pub(crate) fn paint_column_loading(&self, window: &mut gpui::Window, _cx: &gpui::App) {
        if self.shown_view() != super::super::startup::View::Columns
            || self.slots.covers[0].is_some()
        {
            return;
        }
        let Some(columns) = self.panes[0]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
        else {
            return;
        };
        for track in columns
            .area
            .tracks()
            .iter()
            .filter(|track| track.state == TrackState::Loading)
        {
            if let Some(mark) = &columns.loading[track.slot.index()] {
                mark.paint(window);
            }
        }
    }
}

/// A peek track's tile fills the track from its corner: an in-flow child would stand below the 100 percent tall Element.
fn tile() -> gpui::Div {
    div().absolute().top_0().left_0().size_full()
}

/// The loading mark over its line, centred in the track it fills: a peek's tile, or the active track once its listing fell back.
fn loading_tile(
    mark: Rc<crate::feedback::spinner::Crawl>,
    typeface: &crate::list::Typeface,
    ink: gpui::Hsla,
    line: gpui::SharedString,
) -> gpui::Div {
    tile()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(crate::feedback::spinner::GAP))
        .child(crate::feedback::spinner::Slot(mark))
        .child(
            div()
                .text_size(typeface.caption)
                .font_family(typeface.font.family.clone())
                .text_color(ink)
                .child(line),
        )
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
                let mut hero = crate::g2::fb1::hero::EmptyHero::new(catalog, cx);
                hero.set_overlaid();
                hero
            })
        });
        hero.update(cx, |hero, cx| {
            hero.translated(catalog);
            hero.show(columns.area.hero_settled.as_deref() != Some(&view.path), cx);
        });
    } else if let TrackState::Locked(mode) = view.state {
        let notice = columns.notices[at]
            .get_or_insert_with(|| cx.new(|_| crate::g2::fb1::state::StateMessage::new()));
        notice.update(cx, |notice, cx| {
            notice.set(
                Some(crate::g2::fb1::state::Facts::failed(
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
