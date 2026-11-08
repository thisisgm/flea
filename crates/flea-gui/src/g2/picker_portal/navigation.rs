//! Back, Forward and Parent use the chooser's existing folder owner and retain checked identities.
use super::Picker;
use gpui::Context;

// Which way a folder change walks the history, so remember_folder knows which stack it feeds.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Travel {
    #[default]
    Open,
    Back,
    Forward,
}

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
        let parent = flea::ui_logic_picker_listing::parent_of(&self.listing.location).to_owned();
        self.open_folder(&parent, cx);
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        self.travel(Travel::Back, cx);
    }

    // The mouse forward button retraces a Back; with nothing ahead it does nothing.
    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        self.travel(Travel::Forward, cx);
    }

    fn travel(&mut self, travel: Travel, cx: &mut Context<Self>) {
        if self.backend_unavailable() || self.accept_state().submitting() {
            return;
        }
        let stack = match travel {
            Travel::Forward => &mut self.chrome.ahead,
            _ => &mut self.chrome.history,
        };
        let Some(path) = stack.pop() else {
            return;
        };
        self.chrome.travel = travel;
        if flea::ui_logic_picker_listing::is_recent(&path) {
            if let Some(open) = self.chrome.recent_back {
                open(self, cx);
            } else {
                match travel {
                    Travel::Forward => self.chrome.ahead.push(path),
                    _ => self.chrome.history.push(path),
                }
            }
        } else {
            self.open_folder(&path, cx);
        }
        self.chrome.travel = Travel::Open;
    }

    pub(super) fn remember_folder(&mut self, path: &str) {
        if self.listing.location.is_empty() || self.listing.location == path {
            return;
        }
        let here = self.listing.location.clone();
        match self.chrome.travel {
            Travel::Back => self.chrome.ahead.push(here),
            Travel::Forward => self.chrome.history.push(here),
            Travel::Open => {
                self.chrome.history.push(here);
                self.chrome.ahead.clear();
            }
        }
    }
}
