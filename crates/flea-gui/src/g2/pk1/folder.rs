//! New Folder for the modes that may make one, through the existing backend's exclusive mkdir and journal.
use super::Picker;
use flea::{
    json::escape,
    jsondoc::{self, Json},
    ui_logic_pk1 as logic,
};
use gpui::{Context, Window};

impl Picker {
    pub(super) fn open_new_folder(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !logic::can_create_folder(&self.req)
            || !self.listing.location.starts_with('/')
            || self.backend_unavailable()
            || self.accept_state().submitting()
            || self.chrome.folder_request.is_some()
        {
            return;
        }
        let parent = self.listing.location.clone();
        let line = format!(r#"{{"c":"mkdir","path":"{}"}}"#, escape(&parent));
        if self.checks.send(&line) {
            self.chrome.folder_request = Some(parent);
        }
        cx.notify();
    }

    pub(super) fn folder_reply(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Ok(document) = jsondoc::parse(text) else {
            return false;
        };
        let kind = document.get("t").and_then(Json::as_str).unwrap_or_default();
        let refusal =
            kind == "error" && document.get("where").and_then(Json::as_str) == Some("mkdir");
        if kind != "made" && !refusal {
            return false;
        }
        let Some(parent) = self.chrome.folder_request.take() else {
            return true;
        };
        if refusal {
            if let Some(message) = document.get("msg").and_then(Json::as_str) {
                self.say(message.to_owned(), true, cx);
            }
        } else if parent == self.listing.location {
            if let Some(path) = document.get("path").and_then(Json::as_str) {
                if std::path::Path::new(path).parent() == Some(std::path::Path::new(&parent)) {
                    self.open_folder(path, cx);
                }
            }
        }
        cx.notify();
        true
    }
}
