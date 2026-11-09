//! A per-kind view mounted in the Quick Look card: the same views the preview column mounts, under one chrome bar.
use super::glyph::Glyph;
use crate::g2::{
    pv2::{
        PdfView, archive::ArchiveView, code::CodeView, csv::CsvView, font::FontView,
        html::HtmlView, markdown::MarkdownView, office::OfficeView,
    },
    pv3::{
        player::MediaPlayer,
        surface::{Caches, Facts},
    },
    pv4::view::{Model3dView, Phase},
};
use crate::list::Typeface;
use flea::{
    i18n::I18n, ui_logic_pv1::kinds::State, ui_logic_pv1::rect::Shape,
    ui_logic_pv2::refusal::Refusal,
};
use gpui::{App, Entity, EntityId};

/// A media subject: the player, the facts the host knew before it, and the paint caches the card keeps.
#[derive(Clone)]
pub struct Media {
    pub player: Entity<MediaPlayer>,
    pub known: Facts,
    pub caches: Caches,
    pub typeface: Typeface,
    pub video: bool,
}

#[derive(Clone)]
pub enum View {
    Code(Entity<CodeView>),
    Markdown(Entity<MarkdownView>),
    Table(Entity<CsvView>),
    Html(Entity<HtmlView>),
    Archive(Entity<ArchiveView>),
    Office(Entity<OfficeView>),
    Font(Entity<FontView>),
    Pdf(Entity<PdfView>),
    Media(Media),
    Model(Entity<Model3dView>),
}

/// One strip control as the card's bar draws it: its index in the view's own strip, glyph and state.
#[derive(Clone, Debug, PartialEq)]
pub struct StripItem {
    pub index: usize,
    pub glyph: Glyph,
    pub visible: bool,
    pub enabled: bool,
    pub focused: bool,
    pub label: String,
}

/// The bar a mounted view asks for: its tail, its view or sheet or plate words, and the PDF strip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {
    pub tail: Option<String>,
    pub words: Vec<(String, bool)>,
    pub counter: Option<String>,
    pub strip: Vec<StripItem>,
    pub close_focused: bool,
}

/// Why a mounted view draws no content: the sentence, the mark and its tone, and whether Open With shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub sentence: String,
    pub glyph: Glyph,
    pub error: bool,
}

#[derive(Clone)]
pub struct Doc {
    pub view: View,
    pub path: String,
    /// The page a PDF opens at, from the column's expand; cleared once its pages are known.
    pub page: usize,
}

impl PartialEq for Doc {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id() && self.path == other.path
    }
}

impl std::fmt::Debug for Doc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Doc({}, {:?})", self.kind(), self.path)
    }
}

fn strip(items: &[(flea::ui_logic_pv2::viewer::Item, &'static str, String)]) -> Vec<StripItem> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, (item, glyph, label))| {
            Some(StripItem {
                index,
                glyph: Glyph::named(glyph)?,
                visible: item.visible,
                enabled: item.enabled,
                focused: item.focused,
                label: label.clone(),
            })
        })
        .collect()
}

fn pdf_chrome(bar: Option<crate::g2::pv2::Bar>) -> Chrome {
    let bar = bar.unwrap_or(crate::g2::pv2::Bar {
        counter: None,
        items: Vec::new(),
    });
    Chrome {
        counter: bar.counter,
        strip: strip(&bar.items),
        ..Chrome::default()
    }
}

fn words(views: Option<crate::g2::pv2::code::ViewWords>) -> Vec<(String, bool)> {
    use flea::ui_logic_pv2::html::View as Shown;
    views.map_or_else(Vec::new, |w| {
        vec![
            (w.rendered, w.active == Shown::Rendered),
            (w.source, w.active == Shown::Source),
        ]
    })
}

impl Doc {
    pub(crate) fn pinch(&self, delta: f32, cx: &mut gpui::Context<super::QuickLook>) -> bool {
        let pdf = match &self.view {
            View::Pdf(pdf) => Some(pdf.clone()),
            View::Office(office) if !office.read(cx).office.wants_sheets() => {
                Some(office.read(cx).pdf.clone())
            }
            _ => None,
        };
        let Some(pdf) = pdf else {
            return false;
        };
        pdf.update(cx, |pdf, cx| pdf.pinch_by(delta, cx));
        true
    }

    pub fn id(&self) -> EntityId {
        match &self.view {
            View::Code(v) => v.entity_id(),
            View::Markdown(v) => v.entity_id(),
            View::Table(v) => v.entity_id(),
            View::Html(v) => v.entity_id(),
            View::Archive(v) => v.entity_id(),
            View::Office(v) => v.entity_id(),
            View::Font(v) => v.entity_id(),
            View::Pdf(v) => v.entity_id(),
            View::Media(m) => m.player.entity_id(),
            View::Model(v) => v.entity_id(),
        }
    }

    /// Preview.qml kind: the text kinds read as text, as v0.3.12 named them; office and 3D have their own.
    pub fn kind(&self) -> &'static str {
        match &self.view {
            View::Code(_) | View::Markdown(_) | View::Table(_) | View::Html(_) => "text",
            View::Archive(_) => "archive",
            View::Office(_) => "office",
            View::Font(_) => "font",
            View::Pdf(_) => "pdf",
            View::Media(m) if m.video => "video",
            View::Media(_) => "audio",
            View::Model(_) => "model3d",
        }
    }

    /// The card's surface: media, the text kinds and a spreadsheet's table sit on the framed card, pages on the bare background.
    pub fn framed(&self, cx: &App) -> bool {
        match &self.view {
            View::Pdf(_) => false,
            View::Office(v) => v.read(cx).office.wants_sheets(),
            _ => true,
        }
    }

    /// Media draws its strip and no bar; an archive card opens on its own name heading with no bar (ql-archive); every other kind has the chrome bar.
    pub fn has_bar(&self) -> bool {
        !matches!(self.view, View::Media(_) | View::Archive(_))
    }

    /// The bar's data, from the view's own Quick Look bar.
    pub fn chrome(&self, i18n: &I18n, cx: &App) -> Chrome {
        match &self.view {
            View::Code(v) => {
                let bar = v.read(cx).bar(i18n);
                Chrome {
                    tail: bar.count,
                    words: words(bar.views),
                    ..Chrome::default()
                }
            }
            View::Html(v) => {
                let bar = v.read(cx).bar(i18n, cx);
                Chrome {
                    tail: bar.count,
                    words: words(bar.views),
                    ..Chrome::default()
                }
            }
            View::Font(v) => Chrome {
                tail: v.read(cx).bar(i18n).count,
                ..Chrome::default()
            },
            View::Markdown(v) => {
                let bar = v.read(cx).bar(i18n);
                Chrome {
                    tail: bar.count,
                    words: bar.views.into_iter().map(|(_, w, on)| (w, on)).collect(),
                    close_focused: bar.close_focused,
                    ..Chrome::default()
                }
            }
            View::Table(v) => Chrome {
                tail: v.read(cx).count_text(i18n),
                ..Chrome::default()
            },
            View::Office(v) => {
                let bar = v.read(cx).bar(i18n, cx);
                let mut chrome = pdf_chrome(bar.strip);
                chrome.tail = bar.count;
                chrome.words = bar.sheets;
                chrome
            }
            View::Pdf(v) => pdf_chrome(v.read(cx).bar(i18n)),
            View::Model(v) => match v.read(cx).bar() {
                Some(bar) => Chrome {
                    tail: bar.fact,
                    words: if bar.words.len() > 1 {
                        let chosen = bar.chosen;
                        bar.words
                            .into_iter()
                            .enumerate()
                            .map(|(i, w)| (w, i == chosen))
                            .collect()
                    } else {
                        Vec::new()
                    },
                    ..Chrome::default()
                },
                None => Chrome::default(),
            },
            View::Archive(_) | View::Media(_) => Chrome::default(),
        }
    }

    /// The failure the card draws in the view's place, None while the view draws or loads.
    pub fn failure(&self, i18n: &I18n, cx: &App) -> Option<Failure> {
        let error = |sentence: String| Failure {
            sentence,
            glyph: Glyph::Alert,
            error: true,
        };
        match &self.view {
            View::Code(v) => v.read(cx).fail_sentence(i18n).map(error),
            View::Markdown(v) => {
                let view = v.read(cx);
                (view.doc.status == crate::g2::pv2::markdown::Status::Unreadable)
                    .then(|| view.status_sentence().map(str::to_owned))
                    .flatten()
                    .map(error)
            }
            View::Html(v) => v.read(cx).fail_sentence(i18n, cx).map(error),
            View::Archive(v) => v.read(cx).fail_sentence(i18n).map(error),
            View::Font(v) => v.read(cx).fail_sentence(i18n).map(error),
            View::Pdf(v) => v.read(cx).fail_sentence(i18n).map(error),
            View::Office(v) => v.read(cx).failure(i18n).map(|f| Failure {
                sentence: f.sentence,
                glyph: match f.glyph {
                    "lock" => Glyph::Lock,
                    "alert" => Glyph::Alert,
                    _ => Glyph::Row(crate::row::marks::Mark::File),
                },
                error: f.error_tone,
            }),
            View::Model(v) => v.read(cx).failed().map(|sentence| Failure {
                sentence,
                glyph: Glyph::Row(crate::row::marks::Mark::File),
                error: false,
            }),
            View::Table(_) | View::Media(_) => None,
        }
    }

    /// The view has its first screen, its refusal or its failure, so the swap may reveal the card.
    pub fn ready(&self, cx: &App) -> bool {
        match &self.view {
            View::Code(v) => {
                let view = v.read(cx);
                view.first_screen() || view.refusal().is_some()
            }
            View::Markdown(v) => !v.read(cx).loading(cx),
            View::Table(v) => {
                let table = v.read(cx);
                table.first_screen() || table.feed.answer.refused()
            }
            View::Html(v) => {
                let html = v.read(cx);
                html.first_screen(cx) || html.refusal(cx).is_some()
            }
            View::Archive(v) => v.read(cx).meta.is_some(),
            View::Office(v) => v.read(cx).shown(cx),
            View::Font(v) => {
                let font = v.read(cx);
                font.drawn() || font.refusal().is_some()
            }
            View::Pdf(v) => {
                let pdf = v.read(cx);
                pdf.surface.shown_page().is_some() || pdf.surface.failed()
            }
            View::Media(_) => true,
            View::Model(v) => {
                let model = v.read(cx);
                model.phase == Phase::Ready || matches!(model.phase, Phase::Failed(_))
            }
        }
    }

    /// The sentence the calm card draws: a refused code read's binary sentence or v0.3.12's too-large caption, or an office file Flea cannot convert; None otherwise.
    pub fn calm_sentence(&self, i18n: &I18n, cx: &App) -> Option<String> {
        let view = match &self.view {
            View::Code(view) => view,
            View::Office(view) => {
                return view
                    .read(cx)
                    .failure(i18n)
                    .filter(|f| !f.error_tone)
                    .map(|f| f.sentence);
            }
            _ => return None,
        };
        match view.read(cx).refusal()? {
            Refusal::Binary => Some(i18n.tr("preview.unsupported")),
            Refusal::TooLarge => Some(i18n.tr("document.too_large_frame")),
            _ => None,
        }
    }

    /// The card's shape: a page kind sizes to its first page once known, the rest to their kind.
    pub fn shape(&self, state: State, cx: &App) -> Shape {
        if matches!(&self.view, View::Code(view) if view.read(cx).refusal() == Some(Refusal::Binary))
        {
            return Shape::unknown(State::Unsupported);
        }
        let pdf = match &self.view {
            View::Pdf(v) => Some(v.clone()),
            View::Office(v) if !v.read(cx).office.wants_sheets() => Some(v.read(cx).pdf.clone()),
            _ => None,
        };
        let aspect = pdf
            .map(|p| p.read(cx))
            .filter(|p| p.surface.page_count() > 0)
            .map(|p| p.surface.page_aspect());
        match aspect {
            // rect::surface reads only the ratio; a thousand keeps the arithmetic whole.
            Some(aspect) if aspect > 0. => Shape {
                kind: State::Pdf,
                known: true,
                w: 1000. * aspect,
                h: 1000.,
                orient: 1,
            },
            _ => match &self.view {
                View::Office(v) if v.read(cx).office.wants_sheets() => Shape::unknown(State::Text),
                View::Pdf(_) | View::Office(_) => Shape::unknown(State::Pdf),
                _ => Shape::unknown(state),
            },
        }
    }
}
