//! The window's AirDrop: each Act carried out, and the menu and page owly's runs (airdrop_run.rs) feed.
use super::airdrop::Act;
use super::join::now_ms;
use super::seams::Action;
use crate::state::Shell;
use flea::ui_logic_context_menus::build::Context as MenuContext;
use flea::ui_logic_plugin_settings::{airdrop::saved_folder, builtin::AIRDROP};
use flea::ui_logic_send_share::providers::Role;
use gpui::{Context, Window};
use std::process::{Command, Stdio};

// The name Apple devices see, owly's default: this computer's host name.
fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn downloads() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let config = flea::userfile::config_home()
        .unwrap_or_else(|_| std::path::PathBuf::from(&home).join(".config"));
    let body = std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
    download_dir(&body, &home)
}

// Sample input: `XDG_DOWNLOAD_DIR="$HOME/Inbox"`, a user-dirs.dirs line; the last one wins, else ~/Downloads.
fn download_dir(body: &str, home: &str) -> String {
    body.lines()
        .filter_map(|line| crate::navigation::folder_state::user_dir(line, home))
        .filter(|(role, _)| role == "DOWNLOAD")
        .filter_map(|(_, path)| path)
        .next_back()
        .unwrap_or_else(|| format!("{home}/Downloads"))
}

impl Shell {
    // The folder received files go to: the page's choice, else Downloads.
    pub(crate) fn airdrop_folder(&self) -> String {
        let mut plugins = self.declared_menu_plugins("send-target");
        plugins
            .find(|p| p.manifest.id == AIRDROP)
            .and_then(saved_folder)
            .unwrap_or_else(downloads)
    }

    // The facts of a file menu just opened: owly's path, then doctor and a browse while the row shows.
    pub(super) fn airdrop_facts(
        &mut self,
        side: usize,
        owly: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu.providers.airdrop.facts(owly);
        if self.menu.providers.sides[side].row_on(Role::AirDrop) {
            let menus = self.menu.providers.airdrop.menus;
            self.airdrop_look(window, cx, move |shell, seen, window, cx| {
                let acts = (shell.menu.providers.airdrop).menu_looked(now_ms(), seen, menus);
                shell.airdrop_acts(side, acts, window, cx);
            });
        }
    }

    // A shut menu stops the browse; nothing else runs behind it.
    pub(super) fn airdrop_menu_closed(&mut self) {
        for act in self.menu.providers.airdrop.menu_closed() {
            if let Act::Stop(proc) = act {
                self.menu.providers.airdrop_pids.stop(proc);
            }
        }
    }

    // Sample input: {"t":"airdropsent","id":1,"ok":false,"code":"declined","reason":""}
    pub(super) fn airdrop_sent(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let said = (self.menu.providers.airdrop).backend_line(line, &self.locale.catalog);
        self.airdrop_acts(side, said, window, cx);
    }

    pub(crate) fn airdrop_acts(
        &mut self,
        side: usize,
        acts: Vec<Act>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for act in acts {
            match act {
                Act::Doctor(generation, argv) => {
                    self.airdrop_doctor(side, generation, argv, window, cx)
                }
                Act::Start(proc, generation, argv) => {
                    self.airdrop_start(side, proc, generation, argv, window, cx)
                }
                Act::Stop(proc) => self.menu.providers.airdrop_pids.stop(proc),
                Act::Say(text, error) => {
                    self.provider_actions(side, vec![Action::Say(text, error)], window, cx)
                }
                Act::Refresh => {
                    self.provider_refresh(window, cx);
                    self.airdrop_page(cx);
                }
                Act::AskStop => self.airdrop_ask_stop(cx),
            }
        }
        self.airdrop_watch(window, cx);
    }

    // The row shows only while owly is there, the plugin on and the Wi-Fi able, as board 0401-airdrop-menu draws it.
    pub(crate) fn airdrop_context(&self, side: usize, context: &mut MenuContext) {
        let air = &self.menu.providers.airdrop;
        let declared = self
            .declared_menu_plugins("send-target")
            .any(|p| p.manifest.id == AIRDROP);
        context.airdrop_installed =
            declared && self.menu.providers.sides[side].row_on(Role::AirDrop) && air.ready();
        context.airdrop_on = air.on();
        context.airdrop_devices = air.devices();
    }

    // A flyout leaf: Turn On AirDrop, or a device to send the chosen files to.
    pub(crate) fn airdrop_chosen(
        &mut self,
        side: usize,
        leaf: &str,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.locale.catalog.clone();
        let acts = match flea::ui_logic_context_menus::airdrop::peer_id(leaf) {
            Some(peer) => {
                let backend = self.panes[side]
                    .as_ref()
                    .and_then(|host| host.backend.as_ref());
                let wire = super::join::Pipe(backend);
                self.menu
                    .providers
                    .airdrop
                    .send(&wire, peer, paths, &catalog)
            }
            None if leaf == flea::ui_logic_context_menus::airdrop::TURN_ON => {
                let folder = self.airdrop_folder();
                self.airdrop_turn_on(side, &folder, window, cx);
                Vec::new()
            }
            None => Vec::new(),
        };
        self.airdrop_acts(side, acts, window, cx);
    }

    // Receiving on the plugin's page: Everyone for 10 Minutes turns the radio on, Off turns it off.
    pub(crate) fn airdrop_receiving(
        &mut self,
        on: bool,
        folder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = self.focus_side;
        if on {
            self.airdrop_turn_on(side, folder, window, cx);
        } else {
            let acts = self.menu.providers.airdrop.turn_off();
            self.airdrop_acts(side, acts, window, cx);
        }
    }

    // Receiving On withdraws an Off asked of the machine's owner and reads its radio off the window's thread first.
    fn airdrop_turn_on(
        &mut self,
        side: usize,
        folder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu.providers.airdrop.ask_on(folder);
        self.airdrop_probe(true, window, cx, move |shell, seen, window, cx| {
            // A Receiving choice made while the look ran wins over this On.
            let Some(seen) = seen else {
                return;
            };
            let Some(folder) = shell.menu.providers.airdrop.on_wait.take() else {
                return;
            };
            let catalog = shell.locale.catalog.clone();
            let acts = (shell.menu.providers.airdrop).turn_on(&folder, &catalog, seen);
            shell.airdrop_acts(side, acts, window, cx);
        });
    }

    // The plugin switched off while the radio holds Wi-Fi: hand it back at once.
    pub(crate) fn airdrop_plugin_off(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self
            .declared_menu_plugins("send-target")
            .any(|p| p.manifest.id == AIRDROP);
        if !enabled {
            let acts = self.menu.providers.airdrop.turn_off();
            let side = self.focus_side;
            self.airdrop_acts(side, acts, window, cx);
        }
    }

    // The Settings page asks doctor when it shows, so its adapter fact is current.
    pub(crate) fn airdrop_settings_shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The page can open before any file menu brought the backend's facts, so it finds owly itself.
        let path = std::env::var_os("PATH").unwrap_or_default();
        let owly = flea::ui_logic_send_share::airdrop::find(&path).unwrap_or_default();
        self.menu.providers.airdrop.facts(&owly);
        self.airdrop_look(window, cx, |shell, seen, window, cx| {
            let air = &mut shell.menu.providers.airdrop;
            let mut acts = air.look(seen);
            acts.extend(air.ask_doctor(now_ms()));
            let side = shell.focus_side;
            shell.airdrop_acts(side, acts, window, cx);
        });
        self.airdrop_page(cx);
        // The host name is read off the window's thread; the page redraws once it is known.
        cx.spawn_in(window, async move |this, cx| {
            let host = cx.background_executor().spawn(async { host_name() }).await;
            let _ = this.update_in(cx, |shell, _, cx| {
                if shell.menu.providers.airdrop_host != host {
                    shell.menu.providers.airdrop_host = host;
                    shell.airdrop_page(cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn airdrop_page(&mut self, cx: &mut Context<Self>) {
        let page = self
            .menu
            .providers
            .airdrop
            .page(&self.airdrop_folder(), &self.menu.providers.airdrop_host);
        if let Some(view) = &self.settings.plugins.pane {
            view.update(cx, |view, cx| view.set_airdrop(page, cx));
        }
    }

    // Save received files to: Flea's own folder chooser, starting in the folder in effect; esc keeps it.
    pub(crate) fn airdrop_choose_folder(
        &mut self,
        start: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let binary = crate::window::startup::binary();
        let (title, start) = (
            self.locale.catalog.tr("plugins.airdrop.folder"),
            start.to_string(),
        );
        cx.spawn_in(window, async move |this, cx| {
            let chosen = cx
                .background_executor()
                .spawn(async move {
                    let out = Command::new(&binary)
                        .args(["shelf", "choose", &title, &start])
                        .stdin(Stdio::null())
                        .output()
                        .inspect_err(|error| {
                            eprintln!("flea: the folder chooser {binary:?} did not start: {error}")
                        })
                        .ok()?;
                    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    (out.status.success() && path.starts_with('/')).then_some(path)
                })
                .await;
            if let Some(path) = chosen {
                let _ = this.update(cx, |shell, cx| {
                    if let Some(view) = &shell.settings.plugins.pane {
                        view.update(cx, |view, cx| view.set_airdrop_folder(&path, cx));
                    }
                });
            }
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "airdrop_join_tests.rs"]
mod tests;
