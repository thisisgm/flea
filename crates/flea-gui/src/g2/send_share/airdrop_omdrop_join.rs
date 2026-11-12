//! The window's AirDrop when Omdrop runs it: the menu, page and choices of airdrop_join.rs, carried out on Omdrop.
use super::airdrop::Act;
use super::airdrop_omdrop::{DirDone, DirJob};
use super::airdrop_run::Origin;
use super::join::now_ms;
use super::seams::Action;
use crate::state::Shell;
use flea::bytename::{args, text_name};
use flea::ui_logic_context_menus::airdrop::{peer_id, TURN_ON};
use flea::ui_logic_context_menus::build::Context as MenuContext;
use flea::ui_logic_send_share::omdrop::{self, Stack, Tool};
use flea::ui_logic_send_share::providers::Role;
use gpui::{Context, Window};
use std::process::{Command, Stdio};

impl Shell {
    pub(super) fn omdrop_active(&self) -> bool {
        self.menu.providers.omdrop.active()
    }

    // owly runs AirDrop when it is on PATH, else omdrop; the owly path comes back empty while Omdrop runs it.
    // A stack that stops running AirDrop is retired, so nothing it already asked for lands on the other.
    pub(super) fn airdrop_tools(
        &mut self,
        (owly, omdrop): (&str, &str),
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        let omdrop = match omdrop::pick(owly, omdrop) {
            Some(Tool {
                stack: Stack::Omdrop,
                path,
            }) => path,
            _ => String::new(),
        };
        let was_omdrop = self.omdrop_active();
        let om = &self.menu.providers.omdrop;
        let off = (om.path() != omdrop).then(|| om.queued_off()).flatten();
        let retired = self.menu.providers.omdrop.facts(&omdrop);
        if let Some(argv) = off {
            self.omdrop_off_retired(argv, side, window, cx);
        }
        self.stack_acts(Origin::omdrop(side), retired, window, cx);
        if !was_omdrop && self.omdrop_active() {
            let retired = self.menu.providers.airdrop.retire();
            self.airdrop_acts(side, retired, window, cx);
        }
        match omdrop.is_empty() {
            true => owly.to_string(),
            false => String::new(),
        }
    }

    // The Off chosen during `on` is still the user's choice once Omdrop stops running AirDrop: Omdrop's `off` also ends a
    // start still running. Nothing it says lands on the stack that took over, but a refusal is said in the footer.
    fn omdrop_off_retired(
        &mut self,
        argv: Vec<String>,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let run = cx.background_executor().spawn(async move {
                Command::new(text_name(&argv[0]))
                    .args(args(&argv[1..]))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .output()
            });
            let why = match run.await {
                Ok(out) if out.status.success() => return,
                Ok(out) => match omdrop::reason(&String::from_utf8_lossy(&out.stderr)) {
                    why if why.is_empty() => format!("omdrop off: {}", out.status),
                    why => why,
                },
                Err(error) => format!("omdrop off: {error}"),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let said = (shell.locale.catalog)
                    .tr_args("send.providers.airdrop_reason", &[why.as_str()]);
                shell.provider_actions(side, vec![Action::Say(said, true)], window, cx);
            });
        })
        .detach();
    }

    // A file menu just opened: Omdrop's status, then a lookup while the row shows.
    pub(super) fn omdrop_menu_opened(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let enabled = self.airdrop_enabled();
        self.omdrop_plugin(enabled, window, cx);
        if enabled && self.menu.providers.sides[side].row_on(Role::AirDrop) {
            let acts = self.menu.providers.omdrop.menu_opened(now_ms());
            self.stack_acts(Origin::omdrop(side), acts, window, cx);
        }
    }

    pub(super) fn omdrop_context(&self, row_on: bool, context: &mut MenuContext) {
        let om = &self.menu.providers.omdrop;
        context.airdrop_installed = row_on && om.adapter.ready();
        context.airdrop_on = om.on();
        context.airdrop_devices = om.devices();
    }

    // A flyout leaf: Turn On AirDrop, or a device; the send is numbered with owly's, so its answer finds it.
    pub(super) fn omdrop_chosen(
        &mut self,
        side: usize,
        leaf: &str,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.locale.catalog.clone();
        let acts = match peer_id(leaf) {
            Some(peer) => {
                let backend = self.panes[side]
                    .as_ref()
                    .and_then(|host| host.backend.as_ref());
                let wire = super::join::Pipe(backend);
                let providers = &mut self.menu.providers;
                let outbox = &mut providers.airdrop.outbox;
                (providers.omdrop).send(outbox, &wire, peer, paths, &catalog)
            }
            None if leaf == TURN_ON => self.menu.providers.omdrop.turn_on(&catalog),
            None => Vec::new(),
        };
        self.stack_acts(Origin::omdrop(side), acts, window, cx);
    }

    // Receiving on the plugin's page: on runs `omdrop on`, Off runs `omdrop off`.
    pub(super) fn omdrop_receiving(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.locale.catalog.clone();
        let om = &mut self.menu.providers.omdrop;
        let acts = if on {
            om.turn_on(&catalog)
        } else {
            om.receiving_off()
        };
        let side = self.focus_side;
        self.stack_acts(Origin::omdrop(side), acts, window, cx);
    }

    // The plugin switched off stops Flea looking until it is switched on; Omdrop is left as the user set it.
    pub(super) fn omdrop_plugin(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let om = &mut self.menu.providers.omdrop;
        if enabled {
            return om.enable();
        }
        let acts = om.disable();
        let side = self.focus_side;
        self.stack_acts(Origin::omdrop(side), acts, window, cx);
    }

    // A failed status's retry, asked once its delay has passed if nothing made it stale meanwhile.
    pub(super) fn omdrop_status_later(
        &mut self,
        from: Origin,
        token: u64,
        ms: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(ms))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let acts = shell.menu.providers.omdrop.status_due(token);
                shell.stack_acts(from, acts, window, cx);
            });
        })
        .detach();
    }

    // The Settings page asks Omdrop's status when it shows, so its facts are current.
    pub(super) fn omdrop_settings_shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self.airdrop_enabled();
        self.omdrop_plugin(enabled, window, cx);
        let acts = self.menu.providers.omdrop.ask_status();
        let side = self.focus_side;
        self.stack_acts(Origin::omdrop(side), acts, window, cx);
        self.airdrop_page(cx);
    }

    // Omdrop keeps its folder itself, so the choice goes to `omdrop dir`; one chosen while another runs waits for it.
    pub(super) fn omdrop_folder_chosen(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(job) = self.menu.providers.omdrop.folder_argv(path) {
            self.omdrop_dir(job, window, cx);
        }
        self.airdrop_page(cx);
    }

    // One `omdrop dir`: a refusal says why, then the folder chosen meanwhile runs, or Omdrop's status is read again.
    fn omdrop_dir(&mut self, job: DirJob, window: &mut Window, cx: &mut Context<Self>) {
        let DirJob { job, argv } = job;
        cx.spawn_in(window, async move |this, cx| {
            let run = cx.background_executor().spawn(async move {
                Command::new(text_name(&argv[0]))
                    .args(args(&argv[1..]))
                    .stdin(Stdio::null())
                    .output()
            });
            let refused = match run.await {
                Ok(out) if out.status.success() => None,
                Ok(out) => match omdrop::reason(&String::from_utf8_lossy(&out.stderr)) {
                    why if why.is_empty() => Some(format!("omdrop dir: {}", out.status)),
                    why => Some(why),
                },
                Err(error) => Some(format!("omdrop dir: {error}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let catalog = &shell.locale.catalog;
                let mut acts: Vec<Act> = refused
                    .map(|why| {
                        let said =
                            catalog.tr_args("send.providers.airdrop_reason", &[why.as_str()]);
                        Act::Say(said, true)
                    })
                    .into_iter()
                    .collect();
                match shell.menu.providers.omdrop.folder_done(job) {
                    DirDone::Next(next) => shell.omdrop_dir(next, window, cx),
                    DirDone::Asked(asked) => acts.extend(asked),
                }
                let side = shell.focus_side;
                shell.stack_acts(Origin::omdrop(side), acts, window, cx);
            });
        })
        .detach();
    }
}
