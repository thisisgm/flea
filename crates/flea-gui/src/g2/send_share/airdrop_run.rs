//! The window's AirDrop runs as host processes: doctor or status, radio, receiver and browse, and owly's look a second while its radio is up.
use super::airdrop::{Proc, Radio};
use super::airdrop_pids::{Held, Pids};
use super::airdrop_shared::Seen;
use super::join::{now_ms, Senders};
use crate::state::Shell;
use flea::ui_logic_send_share::airdrop_ask;
use flea::ui_logic_send_share::omdrop::Stack;
use gpui::{Context, Window};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::time::Duration;

// How often a window looks at the machine's radio while it is up.
const WATCH: Duration = Duration::from_secs(1);

// Where acts came from: the side that asked, and the stack whose state their runs answer, whichever runs AirDrop by then.
#[derive(Clone, Copy, Debug)]
pub(super) struct Origin {
    pub(super) side: usize,
    pub(super) stack: Stack,
}

impl Origin {
    pub(super) fn omdrop(side: usize) -> Self {
        Origin {
            side,
            stack: Stack::Omdrop,
        }
    }
}

impl Senders {
    // Each stack numbers its runs itself, so each holds them in its own slots.
    pub(super) fn pids(&mut self, stack: Stack) -> &mut Pids {
        match stack {
            Stack::Owly => &mut self.airdrop_pids,
            Stack::Omdrop => &mut self.omdrop_pids,
        }
    }
}

// A run Flea may signal is held in its stack's slots; Omdrop's on and off never are, so no Stop, closing window or quit reaches them.
pub(super) enum Hold {
    Claimed(Held),
    Unowned,
}

impl Hold {
    // Claimed before the spawn, as Pids::claim is.
    pub(super) fn new(slots: &mut Pids, stack: Stack, proc: Proc, generation: u64) -> Self {
        match (stack, proc) {
            (Stack::Omdrop, Proc::Radio) => Hold::Unowned,
            _ => Hold::Claimed(slots.claim(proc, generation)),
        }
    }

    pub(super) fn started(&self, pid: u32) {
        if let Hold::Claimed(run) = self {
            super::airdrop_pids::started(run, pid);
        }
    }

    fn reaped(&self) {
        if let Hold::Claimed(run) = self {
            super::airdrop_pids::reaped(run);
        }
    }
}

impl Shell {
    pub(super) fn airdrop_doctor(
        &mut self,
        from: Origin,
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
                let providers = &mut shell.menu.providers;
                let acts = match from.stack {
                    Stack::Omdrop => {
                        (providers.omdrop).status_done(generation, code, &stdout, now_ms())
                    }
                    Stack::Owly => {
                        (providers.airdrop).doctor_done(generation, code, &stdout, now_ms())
                    }
                };
                shell.stack_acts(from, acts, window, cx);
            });
        })
        .detach();
    }

    // A long run: the radio speaks on stderr, a browse on stdout, the receiver's prompts are answered from a notification.
    pub(super) fn airdrop_start(
        &mut self,
        from: Origin,
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
        let slots = self.menu.providers.pids(from.stack);
        let run = Hold::new(slots, from.stack, proc, generation);
        let owned = matches!(run, Hold::Claimed(_));
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
            run.started(child.id());
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
            run.reaped();
            let _ = lines.send_blocking(Err(code.unwrap_or(-1)));
        });
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(message) = heard.recv().await {
                let done = message.is_err();
                let _ = this.update_in(cx, |shell, window, cx| {
                    let catalog = shell.locale.catalog.clone();
                    let (air, om) = (
                        &mut shell.menu.providers.airdrop,
                        &mut shell.menu.providers.omdrop,
                    );
                    let acts = match (&message, from.stack) {
                        (Ok(line), Stack::Omdrop) => om.line(proc, generation, line),
                        (Ok(line), Stack::Owly) => {
                            air.line(proc, generation, line, now_ms(), &catalog)
                        }
                        (Err(code), Stack::Omdrop) => {
                            om.ended(proc, generation, *code, now_ms(), &catalog)
                        }
                        (Err(code), Stack::Owly) => {
                            air.ended(proc, generation, *code, now_ms(), &catalog)
                        }
                    };
                    if owned && message.is_err() {
                        shell
                            .menu
                            .providers
                            .pids(from.stack)
                            .ended(proc, generation);
                    }
                    shell.stack_acts(from, acts, window, cx);
                });
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    // The machine's radio is read off the window's thread after its earlier asks, and with `claim` a look that finds no radio up takes the machine's lock; None when a Receiving choice came meanwhile.
    pub(super) fn airdrop_probe(
        &mut self,
        keep: bool,
        claim: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Option<&Seen>, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let air = &mut self.menu.providers.airdrop;
        let (done, next) = async_channel::bounded::<()>(1);
        let (probe, asks, before) = (air.shared.probe(), air.asks, air.shared.io.replace(next));
        let (unwritten, (looked, seen)) =
            (keep.then(|| air.shared.hold()), async_channel::bounded(1));
        // The whole chain runs on the background executor, so a quit that ends the event loop never holds it up.
        cx.background_executor()
            .spawn(async move {
                if let Some(before) = before {
                    let _ = before.recv().await;
                }
                if keep {
                    probe.keep();
                }
                drop(unwritten);
                let seen = probe.seen();
                let lock = (claim && seen.radio == Radio::Off)
                    .then(|| probe.claim())
                    .flatten();
                let _ = looked.try_send((seen, lock));
                drop(done);
            })
            .detach();
        cx.spawn_in(window, async move |this, cx| {
            let Ok((seen, lock)) = seen.recv().await else {
                return;
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let air = &mut shell.menu.providers.airdrop;
                air.shared.offer(lock);
                let fresh = air.asks == asks;
                then(shell, fresh.then_some(&seen), window, cx);
                // A lock no step took goes back here, after the window's earlier writes.
                shell.airdrop_write(cx);
            });
        })
        .detach();
    }

    // The machine's state this window changed is written off its thread, after its earlier looks, asks and writes.
    pub(super) fn airdrop_write(&mut self, cx: &mut Context<Self>) {
        let shared = &mut self.menu.providers.airdrop.shared;
        let Some((dir, writes)) = shared.writes() else {
            return;
        };
        let (done, next) = async_channel::bounded::<()>(1);
        let before = shared.io.replace(next);
        cx.background_executor()
            .spawn(async move {
                if let Some(before) = before {
                    let _ = before.recv().await;
                }
                writes.into_iter().for_each(|write| write.run(&dir));
                drop(done);
            })
            .detach();
    }

    // A look that a Receiving choice overtook is taken again, so what it shows is never older than that choice.
    pub(super) fn airdrop_look(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &Seen, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.airdrop_probe(
            false,
            false,
            window,
            cx,
            move |shell, seen, window, cx| match seen {
                Some(seen) => then(shell, seen, window, cx),
                None => shell.airdrop_look(window, cx, then),
            },
        );
    }

    pub(super) fn airdrop_ask_stop(&mut self, cx: &mut Context<Self>) {
        let air = &mut self.menu.providers.airdrop;
        let (done, next) = async_channel::bounded::<()>(1);
        let (probe, before, unwritten) = (
            air.shared.probe(),
            air.shared.io.replace(next),
            air.shared.hold(),
        );
        // Detached, so a stop asked just before the window closes is still written.
        cx.background_executor()
            .spawn(async move {
                if let Some(before) = before {
                    let _ = before.recv().await;
                }
                probe.ask_stop();
                drop(unwritten);
                drop(done);
            })
            .detach();
    }

    // While the radio is up, the owner answers another window's Off and a reader follows the owner's radio.
    pub(super) fn airdrop_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.omdrop_active() || !self.menu.providers.airdrop.watch() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(WATCH).await;
            let (done, going) = async_channel::bounded(1);
            let asked = this.update_in(cx, |shell, window, cx| {
                // Omdrop took AirDrop over, and owly's watch ends with it.
                if shell.omdrop_active() {
                    let _ = done.try_send(false);
                    return;
                }
                // A Receiving On waiting for the radio to come free has this look take the lock.
                let air = &shell.menu.providers.airdrop;
                let claim = air.waits() && air.claims();
                shell.airdrop_probe(false, claim, window, cx, move |shell, seen, window, cx| {
                    // A look a Receiving choice overtook waits for the next second.
                    let Some(seen) = seen else {
                        let _ = done.try_send(true);
                        return;
                    };
                    let catalog = shell.locale.catalog.clone();
                    let (acts, going) = shell.menu.providers.airdrop.tick(now_ms(), &catalog, seen);
                    let side = shell.focus_side;
                    shell.airdrop_acts(side, acts, window, cx);
                    let _ = done.try_send(going);
                });
            });
            if asked.is_err() || !matches!(going.recv().await, Ok(true)) {
                break;
            }
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "airdrop_run_tests.rs"]
mod tests;
