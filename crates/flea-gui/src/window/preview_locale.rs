//! Relabel retained previews on the same snapshot as the rest of their window.
use super::selection_preview::{docs::Doc, Mounted, SelectionPreview};
use crate::state::Shell;
use flea::i18n::I18n;
use gpui::{App, Context};
use std::sync::Arc;

impl Shell {
    pub(crate) fn previews_translated(&mut self, cx: &mut Context<Self>) {
        let catalog = &self.locale.catalog;
        if let Some(look) = &self.quick_look {
            look.view
                .update(cx, |look, cx| look.set_catalog(catalog.clone(), cx));
        }
        for host in self.panes.iter().flatten() {
            if let Some(preview) = &host.preview {
                preview.update(cx, |preview, cx| preview.set_catalog(catalog.clone(), cx));
            }
            host.look_warm.set_catalog(catalog, cx);
        }
    }
}

impl SelectionPreview {
    fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = catalog;
        for frame in self.drawn.iter().chain(self.pending.iter()) {
            match &frame.mounted {
                Mounted::Doc(doc) => doc.set_catalog(&self.catalog, cx),
                Mounted::Model(view) => {
                    view.update(cx, |view, cx| view.set_catalog(self.catalog.clone(), cx))
                }
                Mounted::Pdf(view) => view.update(cx, |_, cx| cx.notify()),
                Mounted::Basic | Mounted::Media(_) => {}
            }
        }
        cx.notify();
    }
}

impl Doc {
    fn set_catalog(&self, catalog: &Arc<I18n>, cx: &mut App) {
        match self {
            Self::Code(view) => view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx)),
            Self::Markdown(view) => view.update(cx, |view, cx| view.set_catalog(catalog, cx)),
            Self::Html(view) => view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx)),
            Self::Archive(view) => {
                view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx))
            }
            Self::Table(view) => view.update(cx, |_, cx| cx.notify()),
            Self::Office(view) => view.update(cx, |_, cx| cx.notify()),
        }
    }
}
