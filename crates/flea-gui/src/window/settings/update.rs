use crate::state::Shell;
use flea::ui_logic_st1::{pages, update};
use gpui::{Context, Window};
use std::{
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl Shell {
    fn sync_settings_update(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.panel.facts.about.update = self.settings.update.clone();
                view.panel.rebuild();
                cx.notify();
            });
        }
    }
    pub(super) fn settings_update(
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
        if action.is_empty() {
            return;
        }
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
