//! IpcPreviewState's Quick Look Markdown, media and PDF-overlay readers; the column's own readers stay in window/selection_preview_readers.rs.
use super::Adapter;
use crate::g2::{
    pv1::{doc::View, QuickLook},
    pv2::{
        code::CodeView,
        markdown::{MarkdownView, View as Shown},
    },
    pv3::sound::MediaSound,
};
use flea::{
    ipc::wire::Reply,
    ui_logic_pv2::markdown::{markdown_layout::FigureState, Block},
};
use gpui::{App, Entity};

pub(crate) struct PreviewLook<'a> {
    pub look: Option<&'a QuickLook>,
    pub cx: &'a App,
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
        let Some(view) = self.markdown() else {
            return String::new();
        };
        let doc = &view.read(self.cx).doc;
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

    fn pdf_overlay(&self) -> bool {
        self.look
            .filter(|look| look.is_open())
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
            .is_some_and(|doc| matches!(doc.view, View::Pdf(_)))
    }
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

/// PreviewMarkdown.qml figureInfo's word: a figure laid out or asked for waits on its drawing, any other is deferred.
fn figure_word(state: Option<FigureState>, busy: bool) -> &'static str {
    match state {
        Some(FigureState::Drawn { .. }) => "ready",
        Some(FigureState::Failed) => "failed",
        Some(FigureState::Waiting) => "working",
        _ if busy => "working",
        _ => "deferred",
    }
}

/// The reader answers for a part GPUI does not yet know, as the IPC contract does for any unported name.
fn not_ported(function: &str) -> Reply {
    Reply::error(3, format!("FLEA_IPC_NOT_PORTED flea.{function}"))
}

impl Adapter for PreviewLook<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let overlay = arguments.first().is_some_and(|flag| flag == "true");
        Some(match function {
            "previewBlockKinds" if overlay => not_ported(function),
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
            "pdfState" if overlay && self.pdf_overlay() => not_ported(function),
            "pdfState" if overlay => Reply::value("null"),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{figure_line, figure_word};
    use flea::ui_logic_pv2::markdown::{markdown_layout::FigureState, Block, Figure, FigureKind};

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
        assert_eq!(figure_word(None, true), "working");
        assert_eq!(figure_word(None, false), "deferred");
    }
}
