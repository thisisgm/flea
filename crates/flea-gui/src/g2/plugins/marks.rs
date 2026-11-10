//! A community plugin's own mark.svg for its rail rows, read once per path off the UI thread.
use super::Host;
use crate::g2::plugin_settings::load::read_mark;
use flea::ui_logic_plugin_settings::wire::Plugin;
use flea::ui_logic_rail_places::rail::OwnedMark;
use gpui::Context;

impl Host {
    pub(super) fn load_marks(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<String> = self
            .plugins
            .iter()
            .map(|p| p.mark_path.clone())
            .filter(|p| !p.is_empty() && !self.marks.contains_key(p))
            .collect();
        for path in wanted {
            self.marks.insert(path.clone(), None);
            let read = path.clone();
            cx.spawn(async move |this, cx| {
                let data = cx
                    .background_executor()
                    .spawn(async move { read_mark(&read) })
                    .await;
                let _ = this.update(cx, |host, cx| {
                    if let Some(data) = data {
                        host.marks.insert(path, Some(data));
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// The plugin's loaded mark, which its rail rows draw in place of their glyph.
    pub(crate) fn mark(&self, plugin: &Plugin) -> Option<OwnedMark> {
        let svg = self.marks.get(&plugin.mark_path)?.clone()?;
        Some(OwnedMark {
            path: plugin.mark_path.clone(),
            svg,
        })
    }
}
