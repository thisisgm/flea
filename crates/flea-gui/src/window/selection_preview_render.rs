//! The approved media frame, optional transport, name and compact or full facts.
use super::*;
pub(crate) use super::fitted::column_picture;
use crate::g2::pv3::{
    self,
    strip::StripElement,
    view::{MediaElement, Picture},
};
use crate::{g2::pv1::glyph::Glyph, row::marks::Mark};
use gpui::{div, prelude::*, px, IntoElement, Render, Window};

/// The frame's border, which a view laid out inside it insets by on each side.
pub(super) const FRAME_BORDER: f32 = 1.;
/// The opacity of a symlink's or an undrawn kind's state mark.
const STATE_MARK_OPACITY: f32 = 0.7;

impl SelectionPreview {
    /// An empty text or code file settled with nothing to draw: its frame stands the document glyph in.
    pub(crate) fn code_blank(&self, frame: &Frame, cx: &gpui::App) -> bool {
        match &frame.mounted {
            Mounted::Doc(crate::window::selection_preview::docs::Doc::Code(view)) => {
                view.read(cx).blank()
            }
            _ => false,
        }
    }
}

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
                let rows = if frame.kind.tall() && !self.full_facts {
                    1
                } else {
                    3
                };
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
            return root.child(self.loading_view(roles.metadata));
        }
        let frame = self.drawn.as_ref().unwrap();
        if frame.manual || !matches!(frame.mounted, Mounted::Media(_)) {
            *self.play_rect.borrow_mut() = None;
        }
        let empty = self.manual_empty(frame);
        let kind = if empty { Kind::Facts } else { frame.kind };
        let full = !frame.kind.tall() || self.full_facts;
        let facts = self.facts(frame, cx);
        let (row, name_row) = self.fact_row();
        let (width, height) = self.frame_size(kind, if full { facts.len() } else { 1 });
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
            .border_color(if empty {
                roles.muted_frame
            } else {
                roles.metadata_frame
            })
            .rounded(px(self.corner))
            .overflow_hidden()
            .child(frame_probe)
            .debug_selector(|| "column-preview-frame".into());
        media = if kind.tall() {
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
        if empty {
            media = media.child(
                state_mark(
                    Glyph::Row(Mark::File),
                    roles.muted_frame,
                    Some(self.catalog.tr("preview.no_preview")),
                )
                .debug_selector(|| "column-preview-manual-empty".into()),
            );
        } else if frame.manual {
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
        } else if self.code_blank(frame, cx) {
            media = media.child(
                state_mark(
                    Glyph::kind(&frame.subject.row.icon, frame.subject.row.mode),
                    roles.muted_frame.opacity(STATE_MARK_OPACITY),
                    None,
                )
                .debug_selector(|| "column-preview-empty-glyph".into()),
            );
        } else {
            let fitted = super::fitted::fitted(frame, width, height);
            let moving = super::picture::moving(frame, width, height);
            let player_failed = matches!(&frame.mounted, Mounted::Media(Some(player))
                if player.read(cx).status() == flea::ui_logic_pv3::player::Status::Failed);
            media = match &frame.mounted {
                Mounted::Pdf(pdf) => media.child(pdf.clone()),
                Mounted::Model(model) => media.child(model.clone()),
                Mounted::Doc(doc) => media.child(doc.element()),
                // v0.3.12 keeps an audio file's cover in the frame through play, pause and seek.
                Mounted::Media(_) if frame.kind == Kind::Audio && fitted.is_some() => {
                    media.children(fitted)
                }
                // PreviewColumn.qml: the column's audio frame is one centred mark; a failed player draws its failure below.
                Mounted::Media(_) if frame.kind == Kind::Audio && !player_failed => media.child(
                    state_mark(
                        Glyph::kind(&frame.subject.row.icon, frame.subject.row.mode),
                        roles.muted_frame.opacity(STATE_MARK_OPACITY),
                        None,
                    )
                    .debug_selector(|| "column-audio-mark".into()),
                ),
                Mounted::Media(player) => {
                    let picture = column_picture(if let Some(player) = player {
                        pv3::surface::picture(
                            player.read(cx),
                            &pv3::surface::Facts::default(),
                            &self.catalog,
                        )
                    } else {
                        // Audio took its mark above, so a frame with no player yet is a video's poster.
                        Picture::Video {
                            frame: frame.image.clone(),
                            poster: true,
                        }
                    });
                    let failed = matches!(picture, Picture::Failed { .. });
                    let element = MediaElement {
                        picture,
                        reserve: 0.,
                        ink,
                        typeface: self.face.clone(),
                        cache: self.caches.view.clone(),
                        painted: player
                            .as_ref()
                            .map_or_else(Default::default, |p| p.read(cx).painted.clone()),
                    };
                    media.child(
                        div()
                            .size_full()
                            .when(failed, |d| {
                                d.debug_selector(|| "column-media-failed".into())
                            })
                            .child(element),
                    )
                }
                _ if frame.kind == Kind::Multi => {
                    media.child(multi::stack(&frame.subject, mark_size, roles.muted_frame))
                }
                _ if moving.is_some() => media.children(moving),
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
        if empty {
            return root;
        }
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
                        .trn("preview.selected", frame.subject.selected_count as i64)
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
