//! The window's AirDrop runs: owly's doctor, radio, receiver and browse as host processes, and the menu and page they feed.
use super::airdrop::{Act, Proc};
use super::join::now_ms;
use super::seams::Action;
use crate::state::Shell;
use flea::ui_logic_mn1::build::Context as MenuContext;
use flea::ui_logic_sd1::{airdrop_ask, providers::Role};
use flea::ui_logic_st2::{airdrop::saved_folder, builtin::AIRDROP};
use gpui::{Context, Window};
use std::io::{BufRead, BufReader, Read};
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
            let acts = self.menu.providers.airdrop.menu_opened(now_ms());
            self.airdrop_acts(side, acts, window, cx);
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
            }
        }
    }

    fn airdrop_doctor(
        &mut self,
        side: usize,
        generation: u64,
        argv: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let program = argv[0].clone();
        cx.spawn_in(window, async move |this, cx| {
            let output = cx
                .background_executor()
                .spawn(async move {
                    Command::new(&argv[0])
                        .args(&argv[1..])
                        .stdin(Stdio::null())
                        .stderr(Stdio::null())
                        .output()
                })
                .await;
            let (code, stdout) = match &output {
                Ok(out) => (
                    out.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&out.stdout).into_owned(),
                ),
                Err(error) => {
                    eprintln!("flea: AirDrop could not run {program} doctor: {error}");
                    (-1, String::new())
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let acts =
                    shell
                        .menu
                        .providers
                        .airdrop
                        .doctor_done(generation, code, &stdout, now_ms());
                shell.airdrop_acts(side, acts, window, cx);
            });
        })
        .detach();
    }

    // A long run: the radio speaks on stderr, a browse on stdout, the receiver's prompts are answered from a notification.
    fn airdrop_start(
        &mut self,
        side: usize,
        proc: Proc,
        generation: u64,
        argv: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match proc {
            Proc::Radio => command.stderr(Stdio::piped()),
            Proc::Browse => command.stdout(Stdio::piped()),
            Proc::Receive => command.stdin(Stdio::piped()).stderr(Stdio::piped()),
        };
        // Ok carries one line of the run's output and Err its exit code, -1 when it never started or died on a signal.
        let (lines, heard) = async_channel::unbounded::<Result<String, i32>>();
        match command.spawn() {
            Ok(mut child) => {
                self.menu
                    .providers
                    .airdrop_pids
                    .hold(proc, generation, child.id());
                let stream: Option<Box<dyn Read + Send>> = match proc {
                    Proc::Radio => child
                        .stderr
                        .take()
                        .map(|s| Box::new(s) as Box<dyn Read + Send>),
                    Proc::Browse => child
                        .stdout
                        .take()
                        .map(|s| Box::new(s) as Box<dyn Read + Send>),
                    Proc::Receive => None,
                };
                let asks = matches!(proc, Proc::Receive)
                    .then(|| child.stderr.take().zip(child.stdin.take()))
                    .flatten();
                let catalog = self.locale.catalog.clone();
                std::thread::spawn(move || {
                    if let Some((stderr, stdin)) = asks {
                        let said = airdrop_ask::serve(stderr, stdin, |ask| {
                            airdrop_ask::ask(&catalog, ask)
                        });
                        for line in said.lines() {
                            let _ = lines.send_blocking(Ok(line.to_string()));
                        }
                    }
                    for line in stream
                        .into_iter()
                        .flat_map(|s| BufReader::new(s).lines().map_while(Result::ok))
                    {
                        let _ = lines.send_blocking(Ok(line));
                    }
                    let code = child.wait().ok().and_then(|status| status.code());
                    let _ = lines.send_blocking(Err(code.unwrap_or(-1)));
                });
            }
            Err(error) => {
                // Said like owly's own error line, so a radio or receiver that cannot start names the program and why.
                let said = format!("owly: {}: {error}", argv[0]);
                eprintln!("flea: AirDrop {said}");
                let _ = lines.send_blocking(Ok(said));
                let _ = lines.send_blocking(Err(-1));
            }
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(message) = heard.recv().await {
                let done = message.is_err();
                let _ = this.update_in(cx, |shell, window, cx| {
                    let catalog = shell.locale.catalog.clone();
                    let acts = match &message {
                        Ok(line) => shell.menu.providers.airdrop.line(proc, generation, line),
                        Err(code) => {
                            shell.menu.providers.airdrop_pids.ended(proc, generation);
                            shell
                                .menu
                                .providers
                                .airdrop
                                .ended(proc, generation, *code, &catalog)
                        }
                    };
                    shell.airdrop_acts(side, acts, window, cx);
                });
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    // The row shows only while owly is there, the plugin on and the Wi-Fi able, as board 0401-airdrop-menu draws it.
    pub(crate) fn airdrop_context(&self, side: usize, context: &mut MenuContext) {
        let air = &self.menu.providers.airdrop;
        let declared = self
            .declared_menu_plugins("send-target")
            .any(|p| p.manifest.id == AIRDROP);
        context.airdrop_installed = declared
            && self.menu.providers.sides[side].row_on(Role::AirDrop)
            && air.adapter.ready();
        context.airdrop_on = air.radio != super::airdrop::Radio::Off;
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
        let acts = match flea::ui_logic_mn1::airdrop::peer_id(leaf) {
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
            None if leaf == flea::ui_logic_mn1::airdrop::TURN_ON => {
                let folder = self.airdrop_folder();
                self.menu.providers.airdrop.turn_on(&folder, &catalog)
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
        let catalog = self.locale.catalog.clone();
        let air = &mut self.menu.providers.airdrop;
        let acts = if on {
            air.turn_on(folder, &catalog)
        } else {
            air.turn_off()
        };
        let side = self.focus_side;
        self.airdrop_acts(side, acts, window, cx);
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
        let owly = flea::ui_logic_sd1::airdrop::find(&path).unwrap_or_default();
        self.menu.providers.airdrop.facts(&owly);
        let acts = self.menu.providers.airdrop.ask_doctor(now_ms());
        let side = self.focus_side;
        self.airdrop_acts(side, acts, window, cx);
        self.airdrop_page(cx);
    }

    pub(crate) fn airdrop_page(&mut self, cx: &mut Context<Self>) {
        let page = self
            .menu
            .providers
            .airdrop
            .page(&self.airdrop_folder(), &host_name());
        if let Some(view) = &self.settings.plugins {
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
                    if let Some(view) = &shell.settings.plugins {
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
