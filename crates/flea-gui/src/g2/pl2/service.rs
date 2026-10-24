//! Runs GIO and bridge checks off the UI thread, with one timer and generation-checked replies.
use super::mount::{Effect, Mounts, Request};
use crate::g2::pl1::process::{Event, Job, Runner, Slot};
use flea::ui_logic_pl1::Note;
use flea::ui_logic_pl2::bridge::{self, Action, Bridge};
use gpui::{Context, EventEmitter, Task};
use std::{rc::Rc, time::Duration};

pub struct ServiceEvent(pub Effect);
struct Wait {
    generation: u64,
    path: String,
    label: String,
    origin: u64,
    flow: Bridge,
}
pub struct Service {
    pub mounts: Mounts,
    pub(super) runner: Rc<dyn Runner>,
    child: Slot,
    child_generation: Option<u64>,
    release: Slot,
    check: Slot,
    check_busy: bool,
    bridge: Option<Wait>,
    pub(super) readers: Vec<Task<()>>,
    pub(super) provider: Slot,
    pub(super) provider_id: Option<u64>,
    pub(super) provider_timer: Option<Task<()>>,
    timer: Option<Task<()>>,
    epoch: Option<std::time::Instant>,
    pub runtime_dir: String,
    pub fuse_binary: String,
}
impl EventEmitter<ServiceEvent> for Service {}
impl Service {
    pub fn new(runner: Rc<dyn Runner>, runtime_dir: String, fuse_binary: String) -> Self {
        Self {
            mounts: Mounts {
                auth_binary: std::env::var("FLEA_GIO_AUTH").unwrap_or_default(),
                ..Mounts::default()
            },
            runner,
            child: Slot::default(),
            child_generation: None,
            release: Slot::default(),
            check: Slot::default(),
            check_busy: false,
            bridge: None,
            readers: Vec::new(),
            provider: Slot::default(),
            provider_id: None,
            provider_timer: None,
            timer: None,
            epoch: None,
            runtime_dir,
            fuse_binary,
        }
    }
    pub fn now(&mut self, cx: &gpui::App) -> u64 {
        let now = cx.background_executor().now();
        let epoch = *self.epoch.get_or_insert(now);
        now.saturating_duration_since(epoch).as_millis() as u64
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.stop_provider();
        self.timer = None;
        self.bridge = None;
        self.mounts.stop_all();
        self.child.kill();
        self.child_generation = None;
        self.release.kill();
        self.check.kill();
        self.check_busy = false;
        let readers = std::mem::take(&mut self.readers);
        cx.spawn(async move |_, _| {
            for reader in readers {
                reader.await;
            }
        })
    }
    pub fn open_share(&mut self, request: Request, now: u64, cx: &mut Context<Self>) {
        let _ = self.now(cx);
        let effects = self.mounts.open_share(request, now);
        self.apply(effects, now, cx);
    }
    pub fn cancel(&mut self, id: u64, now: u64, cx: &mut Context<Self>) {
        let effects = self.mounts.cancel(id);
        self.apply(effects, now, cx);
    }
    pub fn unmount(
        &mut self,
        entry: &flea::ui_logic_pl1::RailEntry,
        listing: &str,
        origin: u64,
        cx: &mut Context<Self>,
    ) {
        if self.release.handle().is_some() {
            return;
        }
        let Some(job) = super::menus::unmount(entry, listing) else {
            return;
        };
        let now = self.now(cx);
        let effects = self.mounts.retire_location(&entry.uri);
        self.apply(effects, now, cx);
        let label = entry.label.clone();
        let handle = self.runner.start(job);
        let events = handle.events.clone();
        let run = self.release.start(handle);
        self.readers.retain(|reader| !reader.is_ready());
        self.readers.push(cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if let Event::Exited(out) = event {
                    let _ = this.update(cx, |this, cx| {
                        if !this.release.ended(run) {
                            return;
                        }
                        cx.emit(ServiceEvent(Effect::RefreshMounts));
                        let note = if out.code == 0 {
                            Note::new("network.unmounted", &[&label])
                        } else {
                            Note::new("network.unmount_failed", &[])
                        };
                        cx.emit(ServiceEvent(Effect::Say { note, origin }));
                    });
                    break;
                }
            }
        }));
    }
    #[allow(clippy::too_many_arguments)]
    pub fn peeked(
        &mut self,
        generation: u64,
        path: &str,
        names: &[String],
        failed: bool,
        login: &str,
        now: u64,
        cx: &mut Context<Self>,
    ) {
        let effects = self
            .mounts
            .peeked(generation, path, names, failed, login, now);
        self.apply(effects, now, cx);
    }
    pub(super) fn apply(&mut self, effects: Vec<Effect>, now: u64, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                Effect::Run {
                    generation,
                    phase,
                    job,
                } => {
                    let handle = self.runner.start(job);
                    self.child_generation = Some(generation);
                    let events = handle.events.clone();
                    let run = self.child.start(handle);
                    self.readers.retain(|task| !task.is_ready());
                    self.readers.push(cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            if let Event::Exited(out) = event {
                                let _ = this.update(cx, |this, cx| {
                                    if this.child.ended(run) {
                                        this.child_generation = None;
                                        let now = this.now(cx);
                                        let effects =
                                            this.mounts.exited(generation, phase, &out, now);
                                        this.apply(effects, now, cx);
                                    }
                                });
                                break;
                            }
                        }
                    }));
                }
                Effect::Kill(generation) => {
                    if self.child_generation == Some(generation) {
                        self.child.kill();
                        self.child_generation = None;
                    }
                    if self
                        .bridge
                        .as_ref()
                        .is_some_and(|b| b.generation == generation)
                    {
                        self.bridge = None;
                        self.check.kill();
                        self.check_busy = false;
                    }
                }
                Effect::EnsureBridge {
                    generation,
                    path,
                    label,
                    origin,
                } => {
                    let dir = bridge::bridge_dir(&self.runtime_dir);
                    if !bridge::needs_bridge(&path, &dir) {
                        let effects = self.mounts.bridge_ready(generation, &path, now);
                        self.apply(effects, now, cx);
                        continue;
                    }
                    if self.bridge.is_some() {
                        self.mounts.bridge_failed(generation);
                        if self.bridge.as_ref().is_some_and(|b| b.path == path) {
                            continue;
                        }
                        cx.emit(ServiceEvent(Effect::Say {
                            note: Note::new("network.another_opening", &[]),
                            origin,
                        }));
                        continue;
                    }
                    let mut flow = Bridge::default();
                    let first = flow.ensure(now);
                    let label = bridge::display_name(&label, &path);
                    self.bridge = Some(Wait {
                        generation,
                        path,
                        label,
                        origin,
                        flow,
                    });
                    self.bridge_actions(vec![first], now, cx);
                }
                other => cx.emit(ServiceEvent(other)),
            }
        }
        self.arm(now, cx);
    }
    fn bridge_actions(&mut self, actions: Vec<Action>, now: u64, cx: &mut Context<Self>) {
        for action in actions {
            let Some(wait) = self.bridge.as_ref() else {
                continue;
            };
            let (generation, path, label, origin) = (
                wait.generation,
                wait.path.clone(),
                wait.label.clone(),
                wait.origin,
            );
            match action {
                Action::CheckDirectory | Action::Classify | Action::CheckFile | Action::Poll => {
                    if self.check_busy {
                        continue;
                    }
                    self.check_busy = true;
                    let flag = if action == Action::CheckFile || action == Action::Poll {
                        "-e"
                    } else {
                        "-d"
                    };
                    let job = Job::plain(&["test", flag, &path]);
                    let handle = self.runner.start(job);
                    let events = handle.events.clone();
                    let run = self.check.start(handle);
                    self.readers.retain(|task| !task.is_ready());
                    self.readers.push(cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            if let Event::Exited(out) = event {
                                let _ = this.update(cx, |this, cx| {
                                    if !this.check.ended(run) {
                                        return;
                                    }
                                    this.check_busy = false;
                                    let now = this.now(cx);
                                    if let Some(wait) =
                                        this.bridge.as_mut().filter(|b| b.generation == generation)
                                    {
                                        let next = wait.flow.checked(out.code == 0, now);
                                        this.bridge_actions(next.into_iter().collect(), now, cx);
                                        this.arm(now, cx);
                                    }
                                });
                                break;
                            }
                        }
                    }));
                }
                Action::Start => {
                    let argv = bridge::fuse_argv(
                        &self.fuse_binary,
                        &bridge::bridge_dir(&self.runtime_dir),
                    );
                    let _ = std::thread::Builder::new()
                        .name("flea-gvfs-daemon".into())
                        .spawn(move || {
                            use std::os::unix::process::CommandExt;
                            use std::process::{Command, Stdio};
                            let mut command = Command::new(&argv[0]);
                            command
                                .args(&argv[1..])
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .process_group(0);
                            if let Ok(mut child) = command.spawn() {
                                let _ = child.wait();
                            }
                        });
                }

                Action::Ready(is_dir) => {
                    self.bridge = None;
                    self.check.kill();
                    self.check_busy = false;
                    let effects = self
                        .mounts
                        .bridge_ready(generation, &path, now)
                        .into_iter()
                        .map(|e| match e {
                            Effect::Open {
                                path,
                                uri,
                                label,
                                origin,
                            } if !is_dir => Effect::OpenFile {
                                path,
                                uri,
                                label,
                                origin,
                            },
                            other => other,
                        })
                        .collect();
                    self.apply(effects, now, cx);
                }
                Action::Show => cx.emit(ServiceEvent(Effect::Say {
                    note: Note::new("network.bridge_starting", &[&label]),
                    origin,
                })),
                Action::Failed => {
                    self.bridge = None;
                    self.check.kill();
                    self.check_busy = false;
                    self.mounts.bridge_failed(generation);
                    cx.emit(ServiceEvent(Effect::Say {
                        note: Note::new("network.bridge_failed_timeout", &[&label]),
                        origin,
                    }));
                }
                Action::Busy => cx.emit(ServiceEvent(Effect::Say {
                    note: Note::new("network.another_opening", &[]),
                    origin,
                })),
            }
        }
    }
    fn arm(&mut self, now: u64, cx: &mut Context<Self>) {
        let due = [
            self.mounts.deadline(),
            self.bridge.as_ref().and_then(|b| b.flow.deadline()),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(due) = due else {
            self.timer = None;
            return;
        };
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(due.saturating_sub(now)))
                .await;
            let _ = this.update(cx, |this, cx| {
                let now = this.now(cx);
                let actions = this
                    .bridge
                    .as_mut()
                    .map(|b| b.flow.tick(now))
                    .unwrap_or_default();
                this.bridge_actions(actions, now, cx);
                let effects = this.mounts.tick(now);
                this.apply(effects, now, cx);
            });
        }));
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.provider.kill();
        self.child.kill();
        self.release.kill();
        self.check.kill();
    }
}
