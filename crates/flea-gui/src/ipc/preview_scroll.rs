//! IpcPreviewState.previewScrollY reads the shown Markdown view in Quick Look.
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
        if function != "previewScrollY" {
            return None;
        }
        let at = self
            .look
            .filter(|look| look.is_open())
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
            .and_then(|doc| match &doc.view {
                View::Markdown(view) => {
                    let view = view.read(self.cx);
                    Some(match view.shown_view() {
                        markdown::View::Rendered => view.scroll,
                        markdown::View::Source => view.source.scroll_y(),
                    })
                }
                _ => None,
            });
        Some(Reply::value(at.map_or_else(
            || "-1".into(),
            |y| (y + 0.5).floor().to_string(),
        )))
    }
}
