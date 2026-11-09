use crate::state::Shell;
use flea::ui_logic_settings_pages::{pages, update};
use gpui::{Context, Window};
use std::{
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl Shell {
    /// The version the background menu's Update Flea row names, empty unless a check found a newer build.
    pub(crate) fn menu_update_version(&self) -> String {
        update::menu_version(&self.settings.update)
    }

    pub(super) fn sync_settings_update(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.panel.facts.about.update = self.settings.update.clone();
                view.panel.rebuild();
                cx.notify();
            });
        }
    }
    pub(crate) fn settings_update(
        &mut self,
        manual: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.updating {
            return;
        }
        let automatic = pages::not_false(&self.launch.settings.data, &["updates", "autoCheck"]);
        let action = if manual {
            update::on_activate(&self.settings.update)
        } else if update::due(&self.settings.update, now(), automatic) {
            "check"
        } else {
            ""
        };
        if !action.is_empty() {
            self.run_update(action, window, cx);
        }
    }

    // UpdateCheck.startPolling: a minute after the window opens, then every six hours, while the switch is on.
    pub(crate) fn start_update_polling(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.settings.poll = Some(cx.spawn_in(window, async move |this, cx| {
            let mut first = true;
            loop {
                let wait = Duration::from_millis(update::poll_wait(first));
                cx.background_executor().timer(wait).await;
                let beat = this.update_in(cx, |shell, window, cx| {
                    let automatic =
                        pages::not_false(&shell.launch.settings.data, &["updates", "autoCheck"]);
                    if !shell.settings.updating
                        && update::poll_asks(&shell.settings.update, now(), automatic, first)
                    {
                        shell.run_update("check", window, cx);
                    }
                });
                if beat.is_err() {
                    return;
                }
                first = false;
            }
        }));
    }

    fn run_update(&mut self, action: &'static str, window: &Window, cx: &mut Context<Self>) {
        self.settings.updating = true;
        if action == "check" {
            self.settings.update = update::checking(&self.settings.update);
        }
        self.sync_settings_update(cx);
        let binary = crate::window::startup::binary();
        cx.spawn_in(window, async move |this, cx| {
            let output = cx
                .background_executor()
                .spawn(async move {
                    let mut command = Command::new(binary);
                    command.arg("--update");
                    if action == "check" {
                        command.arg("check");
                    }
                    command.output()
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                shell.settings.updating = false;
                shell.settings.update = if action == "check" {
                    let text = output
                        .ok()
                        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                        .unwrap_or_default();
                    update::answered(&shell.settings.update, &text, now())
                } else {
                    let started = output.is_ok_and(|out| out.status.success());
                    if let Some(host) = &mut shell.panes[shell.focus_side] {
                        host.status = update::launch_sentence(started)
                            .0
                            .resolve(&shell.locale.catalog)
                            .into();
                    }
                    update::launched_from(&shell.settings.update, started)
                };
                shell.sync_settings_update(cx);
            });
        })
        .detach();
    }
}
