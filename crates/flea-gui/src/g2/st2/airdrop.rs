//! The AirDrop page's window facts (board 0401-airdrop-page): what the window knows, and the two rows the window acts on.
use super::page::PluginsPage;
use super::seam::{Effect, PluginsWire};
use super::view::PluginsPane;
use flea::jsondoc::Json;
use flea::ui_logic_st2::airdrop::{saved_folder, Page, RECEIVING};
use flea::ui_logic_st2::builtin::AIRDROP;
use gpui::Context;

impl PluginsPage {
    // The window's facts with the folder this listing saved, so a new choice shows before the window hears of it.
    pub(super) fn airdrop_page(&self) -> Page {
        let saved = self
            .plugins
            .iter()
            .find(|p| p.manifest.id == AIRDROP)
            .and_then(saved_folder);
        Page {
            folder: saved.unwrap_or_else(|| self.airdrop.folder.clone()),
            ..self.airdrop.clone()
        }
    }

    // Receiving is the window's radio, not a saved setting: the host turns it on or off.
    pub(super) fn airdrop_receiving(&mut self, value: &str) {
        let on = value == RECEIVING[1];
        if on != self.airdrop.receiving {
            self.effect = Effect::AirdropReceiving(on, self.airdrop_page().folder);
        }
    }

    // Board 0401-airdrop-page sets its facts at the right edge; plugin-git-page keeps a plugin's source after its label.
    pub(super) fn facts_right(&self) -> bool {
        self.open_plugin()
            .is_some_and(|plugin| plugin.manifest.id == AIRDROP)
    }

    pub fn set_airdrop(&mut self, page: Page) -> bool {
        if page == self.airdrop {
            return false;
        }
        self.airdrop = page;
        self.rebuild();
        true
    }

    pub fn set_airdrop_folder(&mut self, path: &str, wire: &dyn PluginsWire) {
        self.request(
            wire,
            "set",
            AIRDROP,
            vec![
                ("key", Json::Str("folder".into())),
                ("value", Json::Str(path.into())),
            ],
        );
    }

    #[cfg(test)]
    pub fn airdrop_facts(&self) -> &Page {
        &self.airdrop
    }

    pub(super) fn take_effect(&mut self) -> Effect {
        std::mem::replace(&mut self.effect, Effect::None)
    }
}

impl PluginsPane {
    pub fn set_airdrop(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page.set_airdrop(page) {
            cx.notify();
        }
    }

    pub fn set_airdrop_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        self.page.set_airdrop_folder(path, self.wire.as_ref());
        cx.notify();
    }
}
