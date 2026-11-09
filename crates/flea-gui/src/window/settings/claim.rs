use crate::{g2::settings_pages::claim::Claim, state::Shell};
use flea::ui_logic_settings_pages::{rows::Role, Text};
use gpui::{Context, Window};
use std::{path::PathBuf, process::Command, sync::Mutex};

#[cfg(test)]
#[path = "claim_tests.rs"]
mod tests;

// DefaultClaim.qml readBusy/readAgain: a claim's fresh read waits for the outstanding About read.
static READ: Mutex<()> = Mutex::new(());

fn handler() -> String {
    let _one = READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Command::new("xdg-mime")
        .args(["query", "default", "inode/directory"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().into())
        .unwrap_or_default()
}
fn packaged() -> bool {
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")));
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(std::env::split_paths(&dirs))
        .any(|dir| {
            dir.join("applications")
                .join(crate::g2::settings_pages::claim::DESKTOP)
                .is_file()
        })
}
pub(super) fn completed(mut claim: Claim, binary: PathBuf) -> Claim {
    let mut command = Command::new(&binary);
    command.arg("--default");
    if !claim.claiming {
        command.arg("off");
    }
    match command.output() {
        Err(_) => {
            claim.note = Some((
                Text::args(
                    "settings.default.unstarted",
                    vec![Text::data(
                        binary.file_name().unwrap_or_default().to_string_lossy(),
                    )],
                ),
                Role::Error,
            ))
        }
        Ok(output) => {
            let status = output.status.code().unwrap_or(1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let read = std::thread::spawn(handler);
            let restart = output
                .status
                .success()
                .then(|| std::thread::spawn(flea::chooser_startup::restart));
            claim.handler = read.join().unwrap_or_default();
            let restarted = restart.is_none_or(|job| job.join().unwrap_or(false));
            claim.note = if status == 3 {
                claim.packaged = false;
                Some((Text::Key("settings.default.uninstalled"), Role::Foreground))
            } else if status != 0 {
                let first = stderr
                    .lines()
                    .find_map(|s| s.strip_prefix("flea: "))
                    .or_else(|| stderr.lines().find(|s| !s.trim().is_empty()));
                Some((
                    first.map_or_else(
                        || {
                            Text::args(
                                "settings.default.exited",
                                vec![Text::data(status.to_string())],
                            )
                        },
                        |s| Text::data(s.trim()),
                    ),
                    Role::Error,
                ))
            } else if stderr.contains(
                "no file chooser service is installed, so Flea could not become the file chooser",
            ) {
                Some((Text::Key("settings.default.partly"), Role::Foreground))
            } else if !restarted {
                Some((Text::Key("settings.default.portal"), Role::Foreground))
            } else {
                None
            };
        }
    }
    claim.running = false;
    claim
}

impl Shell {
    pub(super) fn sync_settings_claim(&mut self, cx: &mut Context<Self>) {
        let claim = &self.settings.claim;
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.panel.facts.about.handler = claim.handler.clone();
                view.panel.make_default = claim.rows();
                view.panel.rebuild();
                cx.notify();
            });
        }
    }

    pub(super) fn ensure_settings_claim(&mut self, window: &Window, cx: &mut Context<Self>) {
        let about = self.settings.opened
            && self
                .settings
                .panel
                .as_ref()
                .is_some_and(|view| view.read(cx).panel.section == "about");
        if !about {
            self.settings.about_open = false;
            return;
        }
        if self.settings.about_open {
            return;
        }
        self.settings.about_open = true;
        self.settings_update(false, window, cx);
        let revision = self.settings.claim.revision;
        // The entry probe lands on its own, so a handler read still out never keeps the row from About.
        cx.spawn_in(window, async move |this, cx| {
            let packaged = cx.background_executor().spawn(async { packaged() }).await;
            let _ = this.update(cx, |shell, cx| {
                if shell.settings.claim.revision != revision {
                    return;
                }
                shell.settings.claim.packaged = packaged;
                shell.settings.claim.probed = true;
                shell.sync_settings_claim(cx);
            });
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            let (handler, (owner, package)) = cx
                .background_executor()
                .spawn(async { (handler(), super::provenance::probe()) })
                .await;
            let _ = this.update(cx, |shell, cx| {
                shell.settings_owner(&owner, &package, cx);
                if shell.settings.claim.revision == revision {
                    shell.settings.claim.handler = handler;
                }
                shell.sync_settings_claim(cx);
            });
        })
        .detach();
    }

    pub(super) fn toggle_settings_claim(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.settings.claim.start().is_none() {
            return;
        }
        let claim = self.settings.claim.clone();
        let revision = claim.revision;
        self.sync_settings_claim(cx);
        let binary = crate::window::startup::binary();
        cx.spawn_in(window, async move |this, cx| {
            let claim = cx
                .background_executor()
                .spawn(async move { completed(claim, binary) })
                .await;
            let _ = this.update(cx, |shell, cx| {
                if shell.settings.claim.revision == revision {
                    shell.settings.claim = claim;
                    shell.sync_settings_claim(cx);
                }
            });
        })
        .detach();
    }
}
