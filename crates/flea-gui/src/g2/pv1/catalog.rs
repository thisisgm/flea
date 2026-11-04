//! Catalog snapshots follow mounted views without rebuilding their document or interaction state.
use super::{doc::View, QuickLook};
use flea::i18n::I18n;
use gpui::{App, Context};
use std::sync::Arc;

impl View {
    pub(crate) fn set_catalog(&self, catalog: &Arc<I18n>, cx: &mut App) {
        match self {
            Self::Code(view) => view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx)),
            Self::Markdown(view) => view.update(cx, |view, cx| view.set_catalog(catalog, cx)),
            Self::Html(view) => view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx)),
            Self::Archive(view) => {
                view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx))
            }
            Self::Font(view) => view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx)),
            Self::Model(view) => view.update(cx, |view, cx| view.set_catalog(catalog.clone(), cx)),
            Self::Table(view) => view.update(cx, |_, cx| cx.notify()),
            Self::Office(view) => view.update(cx, |_, cx| cx.notify()),
            Self::Pdf(view) => view.update(cx, |_, cx| cx.notify()),
            Self::Media(_) => {}
        }
    }
}

impl QuickLook {
    pub(crate) fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.i18n = catalog;
        for subject in self.subject.iter().chain(self.closing.iter()) {
            if let Some(doc) = subject.body.doc() {
                doc.view.set_catalog(&self.i18n, cx);
            }
        }
        cx.notify();
    }
}
