//! Host process actions keep their existing ordering and reply ownership on the originating pane.
use super::{Action, Shell};
use crate::g2::sd1::seams::RunKind;
use gpui::{Context, Window};
use std::process::{Command, Stdio};

impl Shell {
    // Says go to the footer; runs leave the UI thread and answer back on this side's sender.
    pub(in crate::g2::sd1) fn provider_actions(
        &mut self,
        side: usize,
        actions: Vec<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for action in actions {
            match action {
                Action::Say(text, error) => {
                    self.status_messages(cx, |messages, now| {
                        messages.say(
                            &text,
                            error,
                            "",
                            flea::ui_logic_fb1::status::Hint::None,
                            now,
                        );
                    });
                }
                // A send may run for minutes, so it waits on its own thread and answers through a channel.
                Action::Spawn(argv, failed) => {
                    let (done, outcome) = async_channel::bounded(1);
                    std::thread::spawn(move || {
                        let status = command(&argv)
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                        let _ = done.send_blocking(status.is_ok_and(|status| status.success()));
                    });
                    cx.spawn_in(window, async move |this, cx| {
                        if outcome.recv().await != Ok(true) {
                            let _ = this.update_in(cx, |shell, window, cx| {
                                shell.provider_actions(
                                    side,
                                    vec![Action::Say(failed, true)],
                                    window,
                                    cx,
                                );
                            });
                        }
                    })
                    .detach();
                }
                Action::Run(kind, argv) => self.provider_run(side, kind, argv, window, cx),
            }
        }
        cx.notify();
    }

    fn provider_run(
        &mut self,
        side: usize,
        kind: RunKind,
        argv: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind_is_clipboard = kind == RunKind::Clipboard;
        cx.spawn_in(window, async move |this, cx| {
            let output = cx
                .background_executor()
                .spawn(async move {
                    let mut command = command(&argv);
                    command.stdin(Stdio::null());
                    // wl-copy leaves a server holding its stdout, so only its exit status is read.
                    if kind_is_clipboard {
                        command.stdout(Stdio::null()).stderr(Stdio::null());
                    }
                    command.output()
                })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let catalog = shell.locale.catalog.clone();
                let sender = &mut shell.menu.providers.sides[side];
                let (started, code, stdout, stderr) = match &output {
                    Ok(out) => (
                        true,
                        exit_code(&out.status),
                        String::from_utf8_lossy(&out.stdout).into_owned(),
                        String::from_utf8_lossy(&out.stderr).into_owned(),
                    ),
                    Err(error) => (false, -1, String::new(), error.to_string()),
                };
                let actions = match kind {
                    RunKind::TaildropStatus(generation) => {
                        sender.taildrop_done(generation, code, &stdout, &stderr, &catalog)
                    }
                    RunKind::DropboxStatus(generation) => {
                        sender.dropbox_done(generation, code, &stdout, &stderr, &catalog)
                    }
                    RunKind::ShareLink => sender.sharelink_done(started, code, &stdout, &catalog),
                    RunKind::Clipboard => sender.clipboard_done(started && code == 0, &catalog),
                };
                shell.provider_actions(side, actions, window, cx);
                if matches!(kind, RunKind::TaildropStatus(_) | RunKind::DropboxStatus(_)) {
                    shell.provider_refresh(window, cx);
                }
            });
        })
        .detach();
    }
}

fn command(argv: &[String]) -> Command {
    let mut command = Command::new(argv.first().map_or("", String::as_str));
    command.args(argv.iter().skip(1));
    command
}

// A signal death reads as 128 plus the signal, as a shell and ui/Taildrop.qml's exit code do.
fn exit_code(status: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(-1)
}

#[cfg(test)]
#[path = "bfx_net_process_tests.rs"]
mod bfx_net_tests;
