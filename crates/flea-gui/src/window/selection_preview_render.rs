//! The approved media frame, optional transport, name and compact or full facts.
use super::*;
use crate::g2::pv3::{
    self,
    strip::{StripElement, StripInput},
    view::{MediaElement, Picture},
};
use gpui::{div, img, prelude::*, px, AppContext, IntoElement, Render, Window};

impl SelectionPreview {
    /// A host may provide the decoder seam while this owner keeps playback lifetime.
    pub fn set_media_backend(&mut self, backend: Rc<dyn pv3::engine::Backend>) {
        self.media_backend = Some(backend);
    }

    /// The transport and preview keymap share this entry point.
    pub fn play_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.play(StripInput::Toggle, window, cx);
    }

    pub(crate) fn play(&mut self, input: StripInput, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible || self.pending.is_some() {
            return;
        }
        let Some(frame) = self.drawn.as_mut().filter(|f| !f.manual) else {
            return;
        };
        let Mounted::Media(player) = &mut frame.mounted else {
            return;
        };
        if player.is_none() && input == StripInput::Toggle {
            let backend = self.media_backend.clone().or_else(|| {
                pv3::sandbox::Sandbox::current()
                    .ok()
                    .map(|b| Rc::new(b) as Rc<dyn pv3::engine::Backend>)
            });
            let Some(backend) = backend else { return };
            let entity = cx.new(|cx| MediaPlayer::new(backend, None, cx));
            let subject = pv3::player::Subject {
                path: frame.subject.path().into(),
                video: frame.kind == Kind::Video,
                box_w: (self.width * window.scale_factor()).max(1.) as u32,
                box_h: (self.height * window.scale_factor()).max(1.) as u32,
                autoplay: true,
            };
            frame._watch = Some(cx.observe(&entity, |_, _, cx| cx.notify()));
            entity.update(cx, |player, cx| player.open(subject, cx));
            *player = Some(entity);
        } else if let Some(player) = player {
            player.update(cx, |player, cx| player.strip_input(input, window, cx));
        }
        cx.notify();
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
            .child(outer);
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
        let groups = self.facts(frame, cx);
        let rows = if full {
            groups.iter().map(Vec::len).sum()
        } else {
            1
        };
        let (width, height) = self.frame_size(frame.kind, rows);
        let bounds = self.frame_rect.clone();
        let frame_probe = gpui::canvas(
            |bounds, _, _| bounds,
            move |_, at, _, _| *bounds.borrow_mut() = Some(at),
        )
        .absolute()
        .size_full();
        let mut media = div()
            .relative()
            .w_full()
            .h(px(height))
            .flex_none()
            .border_1()
            .border_color(roles.metadata_frame)
            .rounded(px(self.corner))
            .overflow_hidden()
            .child(frame_probe);
        if frame.manual {
            media = media.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.catalog.tr_args("preview.loadHint", &["Ctrl+Space"])),
            );
        } else if frame.failed {
            media = media.child(MediaElement {
                picture: Picture::Failed {
                    sentence: self.catalog.tr("document.pdf_unreadable"),
                },
                reserve: 0.,
                ink,
                typeface: self.face.clone(),
                cache: self.caches.view.clone(),
                painted: Default::default(),
            });
        } else {
            media = match &frame.mounted {
                Mounted::Pdf(pdf) if !pdf.read(cx).surface.failed() => media.child(pdf.clone()),
                Mounted::Model(model)
                    if !matches!(model.read(cx).phase, crate::g2::pv4::view::Phase::Failed(_)) =>
                {
                    media.child(model.clone())
                }
                Mounted::Media(player) => {
                    let picture = if let Some(player) = player {
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
                        Picture::Audio {
                            name: frame.subject.row.name.clone(),
                            facts: self.compact(frame, cx),
                        }
                    };
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
                _ if frame.image.is_some() => media.child(
                    img(frame.image.clone().unwrap())
                        .size_full()
                        .object_fit(gpui::ObjectFit::Contain),
                ),
                _ if frame.kind == Kind::Text => {
                    media.child(super::super::preview_text::Text::new(
                        frame.lines.clone(),
                        frame.text_cache.clone(),
                        self.face.clone(),
                        roles.foreground,
                    ))
                }
                _ => media.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(roles.metadata)
                        .child(self.catalog.tr("model3d.cannot_preview")),
                ),
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
            .size_full();
            root = root.child(
                div()
                    .relative()
                    .w_full()
                    .h(px(metrics.header))
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
                .w_full()
                .flex_none()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(self.face.body)
                .child(if frame.kind == Kind::Multi {
                    self.catalog
                        .trn("items.count", frame.subject.selected_count as i64)
                } else {
                    frame.subject.row.name.clone()
                }),
        );
        if full {
            let longest = groups
                .iter()
                .flatten()
                .map(|(label, _)| label.chars().count())
                .max()
                .unwrap_or(0);
            let label_width = longest as f32 * f32::from(self.face.caption) * 0.6 + 15.;
            for group in groups {
                let mut table = div().flex().flex_col().gap(px(3.));
                for (label, value) in group.into_iter().filter(|(_, value)| !value.is_empty()) {
                    table = table.child(
                        div()
                            .flex()
                            .w_full()
                            .child(
                                div()
                                    .w(px(label_width))
                                    .flex_none()
                                    .text_color(roles.metadata)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(value),
                            ),
                    );
                }
                root = root.child(table);
            }
        } else {
            root = root.child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(self.compact(frame, cx)),
            );
        }
        let _ = width;
        root
    }
}
