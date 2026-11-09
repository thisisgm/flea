//! The window's owly runs as host processes: doctor, radio, receiver and browse, and the look a second while the radio is up.
use super::airdrop::Proc;
use super::join::now_ms;
use crate::state::Shell;
use flea::ui_logic_send_share::airdrop_ask;
use gpui::{Context, Window};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::time::Duration;

// How often a window looks at the machine's radio while it is up.
const WATCH: Duration = Duration::from_secs(1);

impl Shell {
    pub(super) fn airdrop_doctor(
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
    pub(super) fn airdrop_start(
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
            Proc::Receive => command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        };
        // Ok carries one line of the run's output and Err its exit code, -1 when it never started or died on a signal.
        let (lines, heard) = async_channel::unbounded::<Result<String, i32>>();
        let run = self.menu.providers.airdrop_pids.claim(proc, generation);
        let catalog = self.locale.catalog.clone();
        // The spawn and every read and wait run on this thread, never the window's.
        std::thread::spawn(move || {
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    // Said like owly's own error line, so a radio or receiver that cannot start names the program and why.
                    let said = format!("owly: {}: {error}", argv[0]);
                    eprintln!("flea: AirDrop {said}");
                    let _ = lines.send_blocking(Ok(said));
                    let _ = lines.send_blocking(Err(-1));
                    return;
                }
            };
            super::airdrop_pids::started(&run, child.id());
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
            // The receiver prints each saved file and accepted link on stdout while its stderr carries the asks.
            let saved = matches!(proc, Proc::Receive)
                .then(|| child.stdout.take())
                .flatten()
                .map(|stdout| {
                    let lines = lines.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            let _ = lines.send_blocking(Ok(line));
                        }
                    })
                });
            let asks = matches!(proc, Proc::Receive)
                .then(|| child.stderr.take().zip(child.stdin.take()))
                .flatten();
            if let Some((stderr, stdin)) = asks {
                airdrop_ask::serve(
                    stderr,
                    stdin,
                    |ask| airdrop_ask::ask(&catalog, ask),
                    |line| {
                        let _ = lines.send_blocking(Ok(line));
                    },
                );
            }
            for line in stream
                .into_iter()
                .flat_map(|s| BufReader::new(s).lines().map_while(Result::ok))
            {
                let _ = lines.send_blocking(Ok(line));
            }
            if let Some(saved) = saved {
                let _ = saved.join();
            }
            let code = child.wait().ok().and_then(|status| status.code());
            super::airdrop_pids::reaped(&run);
            let _ = lines.send_blocking(Err(code.unwrap_or(-1)));
        });
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(message) = heard.recv().await {
                let done = message.is_err();
                let _ = this.update_in(cx, |shell, window, cx| {
                    let catalog = shell.locale.catalog.clone();
                    let air = &mut shell.menu.providers.airdrop;
                    let acts = match &message {
                        Ok(line) => air.line(proc, generation, line, now_ms(), &catalog),
                        Err(code) => {
                            let acts = air.ended(proc, generation, *code, now_ms(), &catalog);
                            shell.menu.providers.airdrop_pids.ended(proc, generation);
                            acts
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

    // While the radio is up, the owner answers another window's Off and a reader follows the owner's radio.
    pub(super) fn airdrop_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu.providers.airdrop.watch() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(WATCH).await;
            let going = this.update_in(cx, |shell, window, cx| {
                let catalog = shell.locale.catalog.clone();
                let (acts, going) = shell.menu.providers.airdrop.tick(now_ms(), &catalog);
                let side = shell.focus_side;
                shell.airdrop_acts(side, acts, window, cx);
                going
            });
            if !matches!(going, Ok(true)) {
                break;
            }
        })
        .detach();
    }
}
