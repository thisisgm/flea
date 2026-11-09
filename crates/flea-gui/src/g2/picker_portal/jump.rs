//! Ranked chooser suggestions reuse PathView and the existing listing child.
use super::Picker;
use flea::jsondoc::Json;
use gpui::Context;

impl Picker {
    pub fn jump_settings(&mut self, settings: &Json, cx: &mut Context<Self>) {
        let favourites = crate::path::history::favourites(settings);
        let path = self.browse.path_view(cx);
        path.update(cx, |view, _| view.set_favourites(favourites));
    }

    pub(super) fn read_jump_history(&mut self, cx: &mut Context<Self>) {
        let read = cx.background_executor().spawn(async move {
            crate::window::path_host::load(crate::window::path_host::file()).1
        });
        cx.spawn(async move |this, cx| {
            let paths = read.await;
            let _ = this.update(cx, |picker, cx| {
                if picker.answered() {
                    return;
                }
                let path = picker.browse.path_view(cx);
                path.update(cx, |view, cx| view.history_read(paths, cx));
            });
        })
        .detach();
    }

    pub(super) fn jump_answer(&mut self, document: &Json, cx: &mut Context<Self>) {
        let (Some(answer), Some(window)) = (crate::path::answer(document), self.browse.window)
        else {
            return;
        };
        let path = self.browse.path_view(cx);
        let _ = window.update(cx, |_, window, cx| {
            path.update(cx, |view, cx| {
                if view.claims(&answer) {
                    view.receive(answer, window, cx);
                }
            })
        });
    }

    pub(super) fn recent_alias(&self) -> Vec<crate::path::crumbs::Alias> {
        vec![crate::path::crumbs::Alias {
            path: flea::ui_logic_picker_listing::RECENT.into(),
            label: self.locale.catalog.tr("picker.browser.recent"),
            key: String::new(),
        }]
    }
}
