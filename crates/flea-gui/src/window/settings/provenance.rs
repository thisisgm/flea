use crate::state::Shell;
use flea::ui_logic_st1::{about, update};
use gpui::Context;
use std::process::Command;

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;

// The package that owns the running binary; a failed query reads as no owner, as v0.3.12 AboutFacts and src/update.rs read it.
pub(super) fn owner() -> String {
    let binary = crate::window::startup::binary();
    Command::new("pacman")
        .arg("-Qqo")
        .arg(&binary)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default()
}

// A build no package owns names its source and package the way the board draws them; an owned build keeps its rows.
pub(super) fn unowned(about: &mut about::Facts, owner: &str) {
    if !owner.is_empty() {
        return;
    }
    about.source = about::installed_from("", false);
    about.package = Some(String::new());
}

impl Shell {
    /// The About probe's owner: an unowned build marks the updater unchecked and draws its source rows on the open panel.
    pub(super) fn settings_owner(&mut self, owner: &str, cx: &mut Context<Self>) {
        if !owner.is_empty() {
            return;
        }
        self.settings.update = update::Status {
            state: update::State::Unchecked,
            kind: "unowned".to_owned(),
            ..Default::default()
        };
        self.sync_settings_update(cx);
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                unowned(&mut view.panel.facts.about, owner);
                view.panel.rebuild();
                cx.notify();
            });
        }
    }
}
