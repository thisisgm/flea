//! State and text readers shared by the mounted column document kinds.
use super::docs::Doc;
use crate::g2::doc_preview;

impl Doc {
    /// The one sentence a failed read draws in the frame's error state.
    pub(crate) fn failure(&self, catalog: &flea::i18n::I18n, cx: &gpui::App) -> Option<String> {
        match self {
            Doc::Code(view) => view.read(cx).fail_sentence(catalog),
            Doc::Markdown(view) => view
                .read(cx)
                .status_sentence()
                .filter(|_| view.read(cx).doc.status == doc_preview::markdown::Status::Unreadable)
                .map(str::to_owned),
            Doc::Html(view) => view.read(cx).fail_sentence(catalog, cx),
            Doc::Archive(view) => view.read(cx).fail_sentence(catalog),
            Doc::Office(view) => view.read(cx).failure(catalog).map(|f| f.sentence),
            Doc::Table(view) => {
                use flea::ui_logic_doc_preview::csv_feed::Answer;
                let key = match view.read(cx).feed.answer {
                    Answer::Large => "document.too_large",
                    Answer::Failed => "document.unreadable",
                    _ => return None,
                };
                Some(catalog.tr(key))
            }
        }
    }

    /// The lines the frame shows, for the column's text readers.
    pub(crate) fn lines(&self, cx: &gpui::App) -> Vec<String> {
        match self {
            Doc::Code(view) => view.read(cx).lines.plain_lines(),
            Doc::Archive(view) => view.read(cx).meta.as_ref().map_or(Vec::new(), |m| {
                m.names.iter().map(|(n, _)| n.clone()).collect()
            }),
            _ => Vec::new(),
        }
    }
}
