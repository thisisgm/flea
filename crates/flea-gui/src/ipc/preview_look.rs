//! IpcPreviewState's Quick Look Markdown, media and PDF-overlay readers; the column's own readers stay in window/selection_preview_readers.rs.
use super::Adapter;
use crate::g2::{
    pv1::{doc::View, QuickLook},
    pv2::markdown::{MarkdownView, View as Shown},
};
use flea::{ipc::wire::Reply, ui_logic_pv2::markdown::Block};
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

    fn holds_figure(&self) -> bool {
        self.markdown().is_some_and(|view| {
            view.read(self.cx)
                .doc
                .blocks()
                .iter()
                .any(|block| matches!(block, Block::Figure(_)))
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
            "previewMarkdownView" => Reply::value(self.markdown().map_or("", |view| {
                match view.read(self.cx).shown_view() {
                    Shown::Rendered => "rendered",
                    Shown::Source => "source",
                }
            })),
            "previewDuration" => Reply::value(self.duration().to_string()),
            "previewFigures" if self.holds_figure() => not_ported(function),
            "previewFigures" => Reply::value(""),
            "previewEndGap" if self.markdown().is_some() => not_ported(function),
            "previewEndGap" => Reply::value("-1"),
            "pdfState" if overlay && self.pdf_overlay() => not_ported(function),
            "pdfState" if overlay => Reply::value("null"),
            _ => return None,
        })
    }
}
