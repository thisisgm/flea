use crate::state::Shell;
use flea::ui_logic_settings_pages::{about, update};
use gpui::Context;
use std::process::Command;

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "package_tests.rs"]
mod package_tests;

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

// Both pacman queries and package parsing run on the About probe's background executor.
pub(super) fn probe() -> (String, about::PackageFacts) {
    let owner = owner();
    let package = if owner.is_empty() {
        about::PackageFacts::default()
    } else {
        Command::new("pacman")
            .args(["-Qi", &owner])
            .env("LC_ALL", "C")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| about::package_facts(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or_default()
    };
    (owner, package)
}

// Failed package queries leave package and build facts unknown, as v0.3.12 AboutFacts does.
pub(super) fn apply(about: &mut about::Facts, owner: &str, package: &about::PackageFacts) {
    about.source = about::installed_from(owner, package.signed);
    if owner.is_empty() {
        about.package = Some(String::new());
        return;
    }
    if !package.package.is_empty() {
        about.package = Some(package.package.clone());
    }
    if !package.built.is_empty() {
        about.built.clone_from(&package.built);
    }
}

impl Shell {
    /// Apply the About probe to the open panel, preserving an owned build's updater answer.
    pub(super) fn settings_owner(
        &mut self,
        owner: &str,
        package: &about::PackageFacts,
        cx: &mut Context<Self>,
    ) {
        if owner.is_empty() {
            self.settings.update = update::Status {
                state: update::State::Unchecked,
                kind: "unowned".to_owned(),
                ..Default::default()
            };
            self.sync_settings_update(cx);
        }
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                apply(&mut view.panel.facts.about, owner, package);
                view.panel.rebuild();
                cx.notify();
            });
        }
    }
}
