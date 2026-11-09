//! What a mounted view does with a key, a bar word or a strip control, and how it stops when the card closes.
use super::doc::{Doc, View};
use crate::g2::doc_preview::PdfView;
use flea::ui_logic_doc_preview::{
    html::View as Shown,
    viewer::{page_direction, Action, Effect, CONTROLS},
};
use flea::ui_logic_media_preview::player::MediaKey;
use flea::ui_logic_preview_core::keys::SEEK_MS;
use gpui::{App, Entity};

/// What the card does after the view saw a key or a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The view has no use for it; the card's own rules run.
    Ignored,
    Taken,
    Close,
    /// The PDF viewer's expand flipped; the card follows its flag.
    Expand(bool),
}

fn pdf_effect(pdf: &Entity<PdfView>, effect: Effect, cx: &App) -> Outcome {
    match effect {
        Effect::Close => Outcome::Close,
        Effect::ToggleExpand => Outcome::Expand(
            pdf.read(cx)
                .viewer
                .as_ref()
                .is_some_and(|viewer| viewer.expanded),
        ),
        _ => Outcome::Taken,
    }
}

fn pdf_action(pdf: &Entity<PdfView>, name: &str, cx: &mut App) -> Outcome {
    let Some(action) = Action::from_name(name) else {
        return Outcome::Ignored;
    };
    let effect = pdf.update(cx, |pdf, cx| pdf.action(action, cx));
    pdf_effect(pdf, effect, cx)
}

fn pdf_press(pdf: &Entity<PdfView>, index: usize, cx: &mut App) -> Outcome {
    let Some(&control) = CONTROLS.get(index) else {
        return Outcome::Ignored;
    };
    let effect = pdf.update(cx, |pdf, cx| {
        let effect = pdf
            .viewer
            .as_mut()
            .map_or(Effect::None, |v| v.activate(control));
        match effect {
            Effect::Turn(delta) => pdf.turn(delta, cx),
            Effect::Zoom(_) => {
                pdf.pan = (0., 0.);
                cx.notify();
            }
            _ => cx.notify(),
        }
        effect
    });
    pdf_effect(pdf, effect, cx)
}

impl Doc {
    // An office document's pages, once they stand; its sheets and its idle states are not a PDF.
    pub(super) fn pages(&self, cx: &App) -> Option<Entity<PdfView>> {
        match &self.view {
            View::Pdf(pdf) => Some(pdf.clone()),
            View::Office(office) => {
                let office = office.read(cx);
                (office.office.state == flea::ui_logic_doc_preview::office::State::Ready
                    && !office.office.wants_sheets())
                .then(|| office.pdf.clone())
            }
            _ => None,
        }
    }

    /// The wrap w toggles on this view; None for a view without one.
    pub fn wrap(&self, cx: &App) -> Option<bool> {
        match &self.view {
            View::Code(code) => Some(code.read(cx).wrap),
            View::Html(html) => Some(html.read(cx).wrap),
            View::Markdown(markdown) => Some(markdown.read(cx).wrap),
            _ => None,
        }
    }

    /// One key-table action the card did not keep for itself.
    pub fn action(&self, name: &str, cx: &mut App) -> Outcome {
        if let Some(pdf) = self.pages(cx) {
            return pdf_action(&pdf, name, cx);
        }
        if let Some(direction) = page_direction(name) {
            match &self.view {
                View::Code(view) => view.update(cx, |view, cx| view.page(direction, 0., cx)),
                View::Markdown(view) => view.update(cx, |view, cx| view.page(direction, 0., cx)),
                View::Html(view) => view.update(cx, |view, cx| view.page(direction, 0., cx)),
                View::Table(view) => view.update(cx, |view, cx| view.page(direction, cx)),
                View::Office(view) => {
                    view.update(cx, |view, cx| view.page(direction, cx));
                }
                _ => return Outcome::Ignored,
            }
            return Outcome::Taken;
        }
        let taken = |taken: bool| {
            if taken {
                Outcome::Taken
            } else {
                Outcome::Ignored
            }
        };
        match &self.view {
            View::Code(code) if name == "toggleWrap" => {
                code.update(cx, |code, cx| {
                    let wrap = !code.wrap;
                    code.set_wrap(wrap, cx)
                });
                Outcome::Taken
            }
            View::Html(html) => match name {
                "toggleWrap" => {
                    html.update(cx, |html, cx| {
                        let wrap = !html.wrap;
                        html.set_wrap(wrap, cx)
                    });
                    Outcome::Taken
                }
                "markdownView" => {
                    html.update(cx, |html, cx| {
                        let next = if html.doc.source_view() {
                            Shown::Rendered
                        } else {
                            Shown::Source
                        };
                        html.set_view(next, cx)
                    });
                    Outcome::Taken
                }
                "zoomIn" | "zoomOut" => {
                    let steps = if name == "zoomIn" { 1 } else { -1 };
                    html.update(cx, |html, cx| html.zoom_by(steps, cx));
                    Outcome::Taken
                }
                "zoomFit" => {
                    html.update(cx, |html, cx| html.zoom_fit(cx));
                    Outcome::Taken
                }
                _ => Outcome::Ignored,
            },
            View::Markdown(md) => {
                use crate::g2::doc_preview::markdown::Effect;
                match md.update(cx, |md, cx| md.action(name, cx)) {
                    Effect::Close => Outcome::Close,
                    Effect::None => Outcome::Ignored,
                    _ => Outcome::Taken,
                }
            }
            View::Office(office) => taken(office.update(cx, |o, cx| o.action(name, cx))),
            View::Model(model) => taken(model.update(cx, |m, cx| m.act(name, cx))),
            View::Media(media) => {
                let key = match name {
                    "playPause" => MediaKey::TogglePlay,
                    "mute" => MediaKey::Mute,
                    "seekBack" => MediaKey::Seek(-SEEK_MS),
                    "seekForward" => MediaKey::Seek(SEEK_MS),
                    _ => return Outcome::Ignored,
                };
                media.player.update(cx, |player, cx| player.key(key, cx));
                Outcome::Taken
            }
            _ => Outcome::Ignored,
        }
    }

    /// PreviewKeys.act ScrollDocument: the rendered page scrolls under the cursor keys.
    pub fn scroll(&self, rows: i64, cx: &mut App) {
        if let View::Html(html) = &self.view {
            html.update(cx, |html, cx| html.scroll_rows(rows, cx));
        }
    }

    /// True while the open page is rendered HTML, whose cursor keys scroll it.
    pub fn html_rendered(&self, cx: &App) -> bool {
        matches!(&self.view, View::Html(html) if !html.read(cx).doc.source_view())
    }

    /// A bar word: Rendered or Source, a sheet, or a plate.
    pub fn word(&self, index: usize, cx: &mut App) {
        match &self.view {
            View::Markdown(md) => {
                use crate::g2::doc_preview::markdown::View as Words;
                let wanted = if index == 0 {
                    Words::Rendered
                } else {
                    Words::Source
                };
                md.update(cx, |md, cx| md.set_view(wanted, cx));
            }
            View::Html(html) => {
                let view = if index == 0 {
                    Shown::Rendered
                } else {
                    Shown::Source
                };
                html.update(cx, |html, cx| html.set_view(view, cx));
            }
            View::Office(office) => {
                let at = office
                    .read(cx)
                    .office
                    .sheet_words()
                    .iter()
                    .position(|(_, on)| *on)
                    .unwrap_or(0);
                let (step, count) = if index >= at {
                    ("nextSheet", index - at)
                } else {
                    ("previousSheet", at - index)
                };
                for _ in 0..count {
                    office.update(cx, |office, cx| office.action(step, cx));
                }
            }
            View::Model(model) => {
                let path = self.path.clone();
                model.update(cx, |model, cx| model.show(&path, index, cx));
            }
            _ => {}
        }
    }

    /// A strip control by its index in the view's own strip.
    pub fn press(&self, index: usize, cx: &mut App) -> Outcome {
        match self.pages(cx) {
            Some(pdf) => pdf_press(&pdf, index, cx),
            None => Outcome::Ignored,
        }
    }

    /// The PDF opens at the column's page once its pages are known; true when that turn ran.
    pub fn settle(&mut self, cx: &mut App) -> bool {
        let Some(pdf) = self.pages(cx).filter(|_| self.page > 0) else {
            return false;
        };
        if pdf.read(cx).surface.page_count() == 0 {
            return false;
        }
        let delta = self.page as i64;
        self.page = 0;
        pdf.update(cx, |pdf, cx| pdf.turn(delta, cx));
        true
    }

    /// The card closed or moved on: media stops, every view stops asking.
    pub fn close(&self, cx: &mut App) {
        match &self.view {
            View::Code(v) => v.update(cx, |v, cx| v.set_active(false, cx)),
            View::Markdown(v) => v.update(cx, |v, cx| v.set_active(false, cx)),
            View::Html(v) => v.update(cx, |v, cx| v.set_active(false, cx)),
            View::Archive(v) => v.update(cx, |v, cx| v.set_active(false, cx)),
            View::Font(v) => v.update(cx, |v, cx| v.set_active(false, cx)),
            View::Pdf(v) => v.update(cx, |v, cx| v.close(cx)),
            View::Office(v) => {
                let pdf = v.read(cx).pdf.clone();
                pdf.update(cx, |pdf, cx| pdf.close(cx));
            }
            View::Media(m) => m.player.update(cx, |p, cx| p.close(cx)),
            View::Model(v) => v.update(cx, |v, cx| v.hide(cx)),
            View::Table(_) => {}
        }
    }
}
