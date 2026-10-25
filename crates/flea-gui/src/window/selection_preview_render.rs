//! The approved media frame, optional transport, name and compact or full facts.
use super::*;
use crate::g2::pv3::{
    self,
    strip::StripElement,
    view::{MediaElement, Picture},
};
use crate::{g2::pv1::glyph::Glyph, row::marks::Mark};
use flea::ui_logic_pv1::thumbs;
use gpui::{div, img, prelude::*, px, IntoElement, Render, Window};

/// The frame's border, which a view laid out inside it insets by on each side.
const FRAME_BORDER: f32 = 1.;
/// The opacity of a symlink's or an undrawn kind's state mark.
const STATE_MARK_OPACITY: f32 = 0.7;

pub(crate) fn number(meta: Option<&Json>, key: &str) -> i64 {
    meta.and_then(|meta| meta.get(key))
        .and_then(Json::as_f64)
        .unwrap_or(0.) as i64
}

impl Render for SelectionPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll(window, cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let roles = crate::list::Palette::inherited(&window.text_style(), theme);
        let ink = pv3::ink::MediaInk::inherited(&window.text_style(), theme);
        let metrics = crate::list::Metrics::for_typeface(&self.face);
        let rect = self.rect.clone();
        let frames = self.frames.clone();
        let stamp = super::frame_feedback::Stamp {
            generation: self.drawn.as_ref().map_or(0, |frame| frame.generation),
            loading: self.pending.is_some()
                && (!self.port.0.borrow().holding || self.drawn.is_none()),
            held: self.port.0.borrow().holding && self.drawn.is_some(),
            blank: false,
        };
        let outer = gpui::canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, _| {
                *rect.borrow_mut() = Some(bounds);
                if let Some(id) = window.current_draw_id() {
                    frames.borrow_mut().stage(id, stamp);
                }
            },
        )
        .absolute()
        .size_full();
        let mut root = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .p(px(metrics.pad))
            .text_color(roles.foreground)
            .font_family(self.face.font.family.clone())
            .text_size(self.face.caption)
            .track_focus(&self.focus)
            .debug_selector(|| "column-preview".into())
            .child(outer);
        // A pending document lays out unseen at its frame's size, so the view sends the asks its first screen needs.
        if let Some(frame) = self.pending.as_ref().filter(|f| !f.manual) {
            if let Mounted::Doc(doc) = &frame.mounted {
                let rows = if frame.kind.tall() && !self.full_facts { 1 } else { 3 };
                let (width, height) = self.frame_size(frame.kind, rows);
                root = root.child(
                    div()
                        .absolute()
                        .top(px(metrics.pad + FRAME_BORDER))
                        .left(px(metrics.pad + FRAME_BORDER))
                        .w(px((width - 2. * FRAME_BORDER).max(1.)))
                        .h(px((height - 2. * FRAME_BORDER).max(1.)))
                        .invisible()
                        .child(doc.element()),
                );
            }
        }
        let loading =
            self.pending.is_some() && (!self.port.0.borrow().holding || self.drawn.is_none());
        if loading || self.drawn.is_none() {
            *self.frame_rect.borrow_mut() = None;
            *self.play_rect.borrow_mut() = None;
            let port = self.port.clone();
            let epoch = self.epoch;
            let loading_mark = gpui::canvas(
                |_, _, _| (),
                move |_, _, _, cx| {
                    let now = cx
                        .background_executor()
                        .now()
                        .saturating_duration_since(epoch)
                        .as_millis() as u64;
                    let mut swap = port.0.borrow_mut();
                    swap.loading_at.get_or_insert(now);
                },
            )
            .absolute()
            .size_full();
            return root.child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(roles.metadata)
                    .child(self.catalog.tr("preview.loading"))
                    .child(loading_mark),
            );
        }
        let frame = self.drawn.as_ref().unwrap();
        if !matches!(frame.mounted, Mounted::Media(_)) {
            *self.play_rect.borrow_mut() = None;
        }
        let full = !frame.kind.tall() || self.full_facts;
        let facts = self.facts(frame, cx);
        let (row, name_row) = self.fact_row();
        let (width, height) = self.frame_size(frame.kind, if full { facts.len() } else { 1 });
        let bounds = self.frame_rect.clone();
        let frame_probe = gpui::canvas(
            |bounds, _, _| bounds,
            move |_, at, _, _| *bounds.borrow_mut() = Some(at),
        )
        .absolute()
        .size_full();
        // PreviewColumn.qml: a tall kind fills the column down to the name, so the name and facts are never cut.
        let mut media = div()
            .relative()
            .w_full()
            .border_1()
            .border_color(roles.metadata_frame)
            .rounded(px(self.corner))
            .overflow_hidden()
            .child(frame_probe);
        media = if frame.kind.tall() {
            media.flex_1().min_h_0()
        } else {
            media.h(px(height)).flex_none()
        };
        let mark_size = crate::window::preview_facts::state_mark_size(&self.face);
        let state_mark = |glyph: Glyph, ink: gpui::Hsla, note: Option<String>| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(metrics.gap))
                .child(crate::g2::pv1::bar::mark(glyph, mark_size, ink))
                .children(note.map(|note| div().text_color(roles.metadata).child(note)))
        };
        let failure = self.failure(frame, cx);
        if frame.manual {
            media = media.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.catalog.tr_args("preview.loadHint", &["Ctrl+Space"])),
            );
        } else if let Some(sentence) = failure {
            media = media.child(MediaElement {
                picture: Picture::Failed { sentence },
                reserve: 0.,
                ink,
                typeface: self.face.clone(),
                cache: self.caches.view.clone(),
                painted: Default::default(),
            });
        } else {
            let fitted = frame.image.clone().map(|image| {
                let size = image.size(0);
                let (w, h) = (size.width.0 as f64, size.height.0 as f64);
                let original = frame.facts.as_ref().map_or((0., 0.), |m| (m.w as f64, m.h as f64));
                let limit = thumbs::poster_limit(frame.kind == Kind::Video, w, h, original.0, original.1);
                let scale = thumbs::fit_scale(
                    (width - 2. * FRAME_BORDER).max(1.) as f64,
                    (height - 2. * FRAME_BORDER).max(1.) as f64,
                    w,
                    h,
                    limit,
                ) as f32;
                div()
                    .debug_selector(|| "column-preview-picture".into())
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(image)
                            .w(px((w as f32 * scale).round()))
                            .h(px((h as f32 * scale).round()))
                            .object_fit(gpui::ObjectFit::Fill),
                    )
            });
            media = match &frame.mounted {
                Mounted::Pdf(pdf) => media.child(pdf.clone()),
                Mounted::Model(model) => media.child(model.clone()),
                Mounted::Doc(doc) => media.child(doc.element()),
                // v0.3.12 keeps an audio file's cover in the frame through play, pause and seek.
                Mounted::Media(_) if frame.kind == Kind::Audio && fitted.is_some() => {
                    media.children(fitted)
                }
                Mounted::Media(player) => {
                    let picture = column_picture(if let Some(player) = player {
                        pv3::surface::picture(
                            player.read(cx),
                            &pv3::surface::Facts::default(),
                            &self.catalog,
                        )
                    } else if frame.kind == Kind::Video {
                        Picture::Video {
                            frame: frame.image.clone(),
                            poster: true,
                        }
                    } else {
                        let (kind, tail) = self.compact(frame, cx);
                        Picture::Audio {
                            name: frame.subject.row.name.clone(),
                            facts: kind + &tail,
                        }
                    });
                    media.child(MediaElement {
                        picture,
                        reserve: 0.,
                        ink,
                        typeface: self.face.clone(),
                        cache: self.caches.view.clone(),
                        painted: player
                            .as_ref()
                            .map_or_else(Default::default, |p| p.read(cx).painted.clone()),
                    })
                }
                _ if frame.kind == Kind::Multi => media.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            self.catalog
                                .trn("items.count", frame.subject.selected_count as i64),
                        ),
                ),
                _ if fitted.is_some() => media.children(fitted),
                _ if frame.kind == Kind::Symlink => media.child(state_mark(
                    Glyph::Row(Mark::Symlink),
                    roles.symlink.opacity(STATE_MARK_OPACITY),
                    None,
                )),
                _ if frame.kind == Kind::Facts => media.child(state_mark(
                    Glyph::Row(Mark::File),
                    roles.muted_frame,
                    Some(self.catalog.tr("preview.no_preview")),
                )),
                _ => media.child(state_mark(
                    Glyph::kind(&frame.subject.row.icon, frame.subject.row.mode),
                    roles.muted_frame.opacity(STATE_MARK_OPACITY),
                    frame
                        .thumb_answered
                        .then(|| self.catalog.tr("preview.no_thumb")),
                )),
            };
        }
        root = root.flex().flex_col().gap(px(metrics.gap)).child(media);
        if let Mounted::Media(player) = &frame.mounted {
            let entity = cx.entity().downgrade();
            let on_input = Rc::new(move |input, window: &mut Window, cx: &mut gpui::App| {
                let _ = entity.update(cx, |view, cx| view.play(input, window, cx));
            });
            let at = self.play_rect.clone();
            let record = gpui::canvas(
                |bounds, _, _| bounds,
                move |_, bounds, _, _| *at.borrow_mut() = Some(bounds),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full();
            root = root.child(
                div()
                    .relative()
                    .w_full()
                    .h(px(pv3::ink::size::STRIP))
                    .child(StripElement {
                        view: pv3::surface::column_view(
                            player.as_ref().map(|p| p.read(cx)),
                            number(frame.meta.as_ref(), "ms"),
                            cx,
                        ),
                        ink,
                        typeface: self.face.clone(),
                        cache: self.caches.strip.clone(),
                        on_input,
                    })
                    .child(record),
            );
        }
        root = root.child(
            div()
                .debug_selector(|| "column-preview-name".into())
                .w_full()
                .flex_none()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .h(px(name_row))
                .text_size(self.face.body)
                .line_height(px(name_row))
                .child(if frame.kind == Kind::Multi {
                    self.catalog
                        .trn("items.count", frame.subject.selected_count as i64)
                } else {
                    frame.subject.row.name.clone()
                }),
        );
        if full {
            let (line, label_width, gap) = self.fact_grid();
            let mut table = div()
                .debug_selector(|| "column-preview-facts".into())
                .flex()
                .flex_col()
                .flex_none()
                .gap(px(gap));
            for (index, (label, value)) in facts.into_iter().enumerate() {
                table = table.child(
                    div()
                        .debug_selector(move || format!("column-preview-fact-{index}"))
                        .flex()
                        .w_full()
                        .h(px(row))
                        .pt(px(1.))
                        .line_height(px(line))
                        .child(
                            div()
                                .w(px(label_width))
                                .flex_none()
                                .text_color(roles.metadata)
                                .child(label),
                        )
                        .child(
                            div()
                                .debug_selector(move || format!("column-preview-value-{index}"))
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(value),
                        ),
                );
            }
            root = root.child(table);
        } else {
            let (kind, tail) = self.compact(frame, cx);
            root = root.child(
                div()
                    .debug_selector(|| "column-preview-facts".into())
                    .flex()
                    .flex_none()
                    .w_full()
                    .h(px(row))
                    .line_height(px(self.fact_grid().0))
                    .text_color(roles.metadata)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(kind),
                    )
                    .child(div().flex_none().whitespace_nowrap().child(tail)),
            );
        }
        root
    }
}

/// cols-preview-video.html draws no play mark over the frame; the strip under it carries play.
pub(crate) fn column_picture(picture: Picture) -> Picture {
    match picture {
        Picture::Video { frame, .. } => Picture::Video { frame, poster: false },
        other => other,
    }
}
