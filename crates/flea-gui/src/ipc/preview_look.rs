//! IpcPreviewState's Quick Look Markdown, media and PDF-overlay readers; the column's own readers stay in window/selection_preview_readers.rs.
use super::Adapter;
use crate::{
    g2::{
        doc_preview::{
            code::CodeView,
            markdown::{MarkdownView, View as Shown},
            PdfView,
        },
        media_preview::sound::MediaSound,
        quick_look_overlay::{doc::View, observations::Observations, Hit, QuickLook},
    },
    window::selection_preview::readers::{centre, rect},
};
use flea::{
    ipc::wire::Reply,
    ui_logic_doc_preview::markdown::{markdown_layout::FigureState, Block},
};
use gpui::{App, Bounds, Entity, Pixels};

pub(crate) struct PreviewLook<'a> {
    pub look: Option<&'a QuickLook>,
    pub cx: &'a App,
    /// keyDeliveryState's focus item is the PdfViewer, IpcPreviewState pdfState's p.activeFocus.
    pub pdf_focused: bool,
}

impl PreviewLook<'_> {
    /// The Markdown view Quick Look shows, while the card is open.
    fn markdown(&self) -> Option<&Entity<MarkdownView>> {
        self.look
            .filter(|look| look.is_open())
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
            .and_then(|doc| match &doc.view {
                View::Markdown(view) => Some(view),
                _ => None,
            })
    }

    /// The open card's document view, None while the card is closed or shows no document.
    fn view(&self) -> Option<&View> {
        self.look
            .filter(|look| look.is_open())
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
            .map(|doc| &doc.view)
    }

    /// UTF-16 length of the code or HTML Source text, falling back to held lines; 0 without text.
    fn text_length(&self) -> usize {
        let code = |view: &Entity<CodeView>| {
            let lines = &view.read(self.cx).lines;
            lines.text.as_deref().map_or_else(
                || lines.plain_lines().join("\n").encode_utf16().count(),
                |text| text.encode_utf16().count(),
            )
        };
        match self.view() {
            Some(View::Code(view)) => code(view),
            Some(View::Html(html)) => html.read(self.cx).source.as_ref().map_or(0, code),
            _ => 0,
        }
    }

    /// Milliseconds of the card's media, 0 when the card shows no media.
    fn duration(&self) -> i64 {
        self.look
            .filter(|look| look.is_open())
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
            .map_or(0, |doc| match &doc.view {
                View::Media(media) => media.player.read(self.cx).duration,
                _ => 0,
            })
    }

    /// The top-level block kinds of a ready document: `heading`, `fence:rust`, `list[run,run]`.
    fn block_kinds(&self) -> String {
        self.markdown()
            .map_or_else(String::new, |view| block_kinds(view.read(self.cx)))
    }

    /// IpcPreviewState.previewFigures: `index=state` per top-level figure block, "" until the document is ready.
    fn figures(&self) -> String {
        let Some(view) = self.markdown() else {
            return String::new();
        };
        let doc = &view.read(self.cx).doc;
        if !doc.content_ready() {
            return String::new();
        }
        figure_line(doc.blocks(), |i| {
            let ordinal = doc.counter(i).figure;
            figure_word(
                doc.assets.figures.get(&ordinal).copied(),
                doc.current(i) || doc.figure_asked(ordinal),
            )
        })
    }

    /// The open card's PDF, None while the card is closed or shows another kind.
    fn pdf(&self) -> Option<&PdfView> {
        match self.view() {
            Some(View::Pdf(pdf)) => Some(pdf.read(self.cx)),
            _ => None,
        }
    }

    /// IpcPreviewState pdfState(true): the card's page, zoom and scroll, and each strip control as it draws.
    fn pdf_state(&self) -> String {
        use flea::{jsondoc::Json, ui_logic_doc_preview::viewer::CONTROLS};
        let (Some(look), Some(pdf)) = (self.look, self.pdf()) else {
            return "null".into();
        };
        let Some(viewer) = pdf.viewer.as_ref() else {
            return "null".into();
        };
        let (page, count) = (pdf.surface.page, pdf.surface.page_count());
        let number = |n: f64| Json::Num(n.to_string());
        let painted = look.observations.borrow().strip;
        let hovered = look.observations.borrow().strip_hovered;
        let close = look.observations.borrow().close;
        let close_hovered = look.observations.borrow().close_hovered;
        let i18n = &look.i18n;
        let controls = viewer
            .strip(page, count)
            .iter()
            .zip(CONTROLS)
            .enumerate()
            .map(|(index, (item, control))| {
                // The strip's close is the card's own close control; the rest are strip slots.
                let hit = if control == flea::ui_logic_doc_preview::viewer::Control::Close {
                    Hit::Close
                } else {
                    Hit::Strip(index)
                };
                let ring = item.focused || look.focused() == Some(hit);
                let bounds = if hit == Hit::Close {
                    close
                } else {
                    painted[index]
                };
                Json::Obj(vec![
                    ("kind".into(), Json::Str("ChromeButton".into())),
                    ("rect".into(), Json::Str(rect(bounds))),
                    ("name".into(), Json::Str(i18n.tr(control.name_key()))),
                    ("enabled".into(), Json::Bool(item.enabled)),
                    ("visible".into(), Json::Bool(item.visible)),
                    (
                        "centre".into(),
                        Json::Str(bounds.map_or_else(String::new, centre)),
                    ),
                    (
                        "hovered".into(),
                        Json::Bool(
                            (if hit == Hit::Close {
                                close_hovered
                            } else {
                                hovered[index]
                            }) || look.hovered() == Some(hit),
                        ),
                    ),
                    ("pressed".into(), Json::Bool(look.pressed() == Some(hit))),
                    ("ring".into(), Json::Bool(ring)),
                ])
            })
            .collect();
        flea::jsondoc::render(&Json::Obj(vec![
            ("page".into(), number(page as f64)),
            ("pages".into(), number(count as f64)),
            ("zoom".into(), number(viewer.zoom)),
            ("scrollY".into(), number(pdf.surface.view_top)),
            ("focused".into(), Json::Bool(self.pdf_focused)),
            (
                "control".into(),
                number(viewer.control.map_or(-1., |index| index as f64)),
            ),
            ("controls".into(), Json::Arr(controls)),
        ]))
    }

    /// The open card's painted boxes, None while the card is closed so the column answers.
    fn painted(&self, pick: impl Fn(&Observations) -> Option<Bounds<Pixels>>) -> Option<String> {
        let look = self.look.filter(|look| look.is_open())?;
        Some(rect(pick(&look.observations.borrow())))
    }
}

/// IpcPreviewState.previewBlockKinds for one Markdown view: its top-level block kinds as JSON, "" until the document is ready.
pub(crate) fn block_kinds(view: &MarkdownView) -> String {
    let doc = &view.doc;
    if !doc.content_ready() {
        return String::new();
    }
    let kinds: Vec<String> = doc
        .blocks()
        .iter()
        .map(|block| format!("\"{}\"", flea::json::escape(&kind_of(block))))
        .collect();
    format!("[{}]", kinds.join(","))
}

/// The kind word IpcPreviewState.kindOf writes: the block type, a fence's info tag, and nested part kinds in brackets.
fn kind_of(block: &Block) -> String {
    let inner: Vec<&Block> = match block {
        Block::Quote(quote) => quote.parts.iter().flatten().collect(),
        Block::List(list) => list.parts.iter().flatten().flatten().flatten().collect(),
        _ => Vec::new(),
    };
    if !inner.is_empty() {
        let parts: Vec<String> = inner.into_iter().map(kind_of).collect();
        return format!("{}[{}]", block.kind(), parts.join(","));
    }
    match block {
        Block::Fence { info, .. } if !info.is_empty() => format!("fence:{info}"),
        _ => block.kind().into(),
    }
}

/// One `index=state` entry per top-level figure block, joined by commas.
fn figure_line(blocks: &[Block], state: impl Fn(usize) -> &'static str) -> String {
    blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| matches!(block, Block::Figure(_)))
        .map(|(i, _)| format!("{i}={}", state(i)))
        .collect::<Vec<_>>()
        .join(",")
}

/// PreviewMarkdown.qml figureInfo's word: a figure waiting, laid out or asked for is working; one with no state and neither is deferred.
fn figure_word(state: Option<FigureState>, busy: bool) -> &'static str {
    match state {
        Some(FigureState::Drawn { .. }) => "ready",
        Some(FigureState::Failed) => "failed",
        Some(FigureState::Waiting) => "working",
        _ if busy => "working",
        _ => "deferred",
    }
}

impl Adapter for PreviewLook<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function == "cohesionControls" {
            let look = self.look.filter(|look| look.is_open())?;
            let state = if self.pdf().is_some() {
                let source = flea::jsondoc::parse(&self.pdf_state()).ok()?;
                super::cohesion_controls::from_controls(&source, "Preview")
            } else {
                let drawn = look.observations.borrow();
                let bounds = drawn.close?;
                let source = crate::visual::object([
                    (
                        "name",
                        flea::jsondoc::Json::Str(look.i18n.tr("preview.close")),
                    ),
                    ("rect", flea::jsondoc::Json::Str(rect(Some(bounds)))),
                    ("centre", flea::jsondoc::Json::Str(centre(bounds))),
                    ("visible", flea::jsondoc::Json::Bool(look.is_revealed())),
                    ("enabled", flea::jsondoc::Json::Bool(true)),
                    (
                        "focused",
                        flea::jsondoc::Json::Bool(look.focused() == Some(Hit::Close)),
                    ),
                    (
                        "hovered",
                        flea::jsondoc::Json::Bool(
                            drawn.close_hovered || look.hovered() == Some(Hit::Close),
                        ),
                    ),
                    (
                        "pressed",
                        flea::jsondoc::Json::Bool(look.pressed() == Some(Hit::Close)),
                    ),
                ]);
                super::cohesion_controls::state(
                    super::cohesion_controls::control(&source, "Preview", "ChromeButton")
                        .into_iter()
                        .collect(),
                )
            };
            return Some(Reply::value(flea::jsondoc::render(&state).trim_end()));
        }
        // pdfState(overlay) and previewBlockKinds(column) take one bool each.
        let flag = arguments.first().is_some_and(|flag| flag == "true");
        Some(match function {
            // previewBlockKinds(true) reads the column's Markdown; ColumnsPreview answers it ahead of this adapter.
            "previewBlockKinds" if flag => return None,
            "previewBlockKinds" => Reply::value(self.block_kinds()),
            "previewMarkdownView" => {
                Reply::value(self.markdown().map_or(
                    "",
                    |view| match view.read(self.cx).shown_view() {
                        Shown::Rendered => "rendered",
                        Shown::Source => "source",
                    },
                ))
            }
            "previewDuration" => Reply::value(self.duration().to_string()),
            "previewFigures" => Reply::value(self.figures()),
            // Quick Look's media reader answers a media card; any other card, or none, draws no strip.
            "previewStrip" => Reply::value(format!(
                r#"{{"visible":false,"muted":{},"mute":""}}"#,
                MediaSound::get(self.cx).muted
            )),
            "previewTextLength" => Reply::value(self.text_length().to_string()),
            "pdfState" if flag => Reply::value(self.pdf_state()),
            // IpcPreviewState reads both boxes from Quick Look's surface item; a closed card leaves the column's.
            "previewSurfaceRect" => Reply::value(self.painted(|observed| observed.surface)?),
            "previewPictureRect" => Reply::value(self.painted(|observed| observed.picture)?),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{figure_line, figure_word};
    use flea::ui_logic_doc_preview::markdown::{
        markdown_layout::FigureState, Block, Figure, FigureKind,
    };

    // EY-238: the figure entries name each top-level figure block by index, with the word figureInfo gives.
    #[test]
    fn ey238_figures_name_each_top_level_figure_with_its_state() {
        let figure = || {
            Block::Figure(Figure {
                kind: FigureKind::Mermaid,
                source: "graph TD; A-->B".into(),
                display: true,
            })
        };
        let run = || Block::Run {
            text: "text".into(),
            maths: Vec::new(),
        };
        let blocks = vec![run(), figure(), run(), figure(), figure()];
        let words = ["", "ready", "", "failed", "deferred"];
        assert_eq!(
            figure_line(&blocks, |i| words[i]),
            "1=ready,3=failed,4=deferred"
        );
        assert_eq!(figure_line(&[run()], |_| "ready"), "");
        let drawn = FigureState::Drawn {
            width: 10.,
            height: 10.,
        };
        assert_eq!(figure_word(Some(drawn), false), "ready");
        assert_eq!(figure_word(Some(FigureState::Failed), true), "failed");
        assert_eq!(figure_word(Some(FigureState::Waiting), true), "working");
        assert_eq!(figure_word(Some(FigureState::Waiting), false), "working");
        assert_eq!(figure_word(None, true), "working");
        assert_eq!(figure_word(None, false), "deferred");
    }
}
