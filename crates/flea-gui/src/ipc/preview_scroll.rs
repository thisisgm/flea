//! IpcPreviewState.previewScrollY and previewTable read the shown Markdown view in Quick Look.
use super::Adapter;
use crate::g2::{
    pv1::{doc::View, QuickLook},
    pv2::markdown,
};
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct PreviewScroll<'a> {
    pub look: Option<&'a QuickLook>,
    pub cx: &'a App,
}

impl Adapter for PreviewScroll<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "previewScrollY" && function != "previewTable" {
            return None;
        }
        let shown = self
            .look
            .filter(|look| look.is_open())
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
            .and_then(|doc| match &doc.view {
                View::Markdown(view) => Some(view.read(self.cx)),
                _ => None,
            });
        if function == "previewTable" {
            return Some(Reply::value(
                shown.map_or_else(|| "{}".into(), |view| view.table_state()),
            ));
        }
        // PreviewMarkdown.qml scrollY is the body's contentY, which rests at minus its top inset; GPUI's scroll rests at 0.
        let at = shown.map(|view| match view.shown_view() {
            markdown::View::Rendered => view.scroll - view.tokens.inset_y(),
            markdown::View::Source => view.source.scroll_y(),
        });
        Some(Reply::value(at.map_or_else(
            || "-1".into(),
            |y| (y + 0.5).floor().to_string(),
        )))
    }
}
