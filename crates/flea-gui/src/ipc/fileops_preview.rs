//! The archive index of the live Quick Look subject, never the selection preview's index.
use super::Adapter;
use crate::{g2::quick_look_overlay::doc::View, state::Shell};
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct PreviewAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
}

impl Adapter for PreviewAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "previewArchiveNames" {
            return None;
        }
        let subject = self
            .shell
            .quick_look
            .as_ref()
            .map(|host| host.view.read(self.cx))
            .filter(|view| view.is_open())
            .and_then(|view| view.subject())
            .and_then(|subject| subject.body.doc());
        let names = subject
            .and_then(|doc| match &doc.view {
                View::Archive(archive) => archive.read(self.cx).meta.as_ref(),
                _ => None,
            })
            .map_or_else(String::new, |meta| {
                meta.names
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join("|")
            });
        Some(Reply::value(names))
    }
}
