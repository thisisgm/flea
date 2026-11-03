//! Back and Parent use the chooser's existing folder owner and retain checked identities.
use super::Picker;
use gpui::Context;

impl Picker {
    pub fn can_go_up(&self) -> bool {
        !self.backend_unavailable()
            && !self.accept_state().submitting()
            && self.listing.location.starts_with('/')
            && self.listing.location != "/"
    }

    pub fn go_up(&mut self, cx: &mut Context<Self>) {
        if !self.can_go_up() {
            return;
        }
        let parent = flea::ui_logic_pk2::parent_of(&self.listing.location).to_owned();
        self.open_folder(&parent, cx);
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.backend_unavailable() || self.accept_state().submitting() {
            return;
        }
        let Some(path) = self.chrome.history.pop() else {
            return;
        };
        self.chrome.backing = true;
        if flea::ui_logic_pk2::is_recent(&path) {
            if let Some(open) = self.chrome.recent_back {
                open(self, cx);
            } else {
                self.chrome.history.push(path);
            }
        } else {
            self.open_folder(&path, cx);
        }
        self.chrome.backing = false;
    }

    pub(super) fn remember_folder(&mut self, path: &str) {
        if !self.chrome.backing
            && !self.listing.location.is_empty()
            && self.listing.location != path
        {
            self.chrome.history.push(self.listing.location.clone());
        }
    }
}
