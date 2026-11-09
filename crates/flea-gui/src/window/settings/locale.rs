use crate::state::Shell;
use gpui::Context;

impl Shell {
    pub(crate) fn settings_translated(&mut self, cx: &mut Context<Self>) {
        let catalog = self.locale.catalog.clone();
        if let Some(view) = &self.settings.messages {
            view.update(cx, |view, cx| view.translated(catalog.clone(), cx));
        }
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.panel.facts.language = catalog.locale().name.clone();
                view.panel.set_catalog(catalog.clone());
                cx.notify();
            });
        }
        if let Some(view) = &self.settings.sheet {
            view.update(cx, |view, cx| {
                view.sheet.set_catalog(catalog.clone());
                cx.notify();
            });
        }
        if let Some(view) = &self.settings.plugins.pane {
            view.update(cx, |view, cx| view.set_catalog(catalog, cx));
        }
        self.sync_trash_sheet(cx);
    }
}
