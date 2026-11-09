//! The figure helper and store as processes, one pair for the app: an ask becomes a ticket, and every answer comes back as an event each view reads.
use flea::ui_logic_doc_preview::markdown::{
    markdown_figure::Theme,
    markdown_figure_service::{Command, Service},
    FigureKind,
};
use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Task};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// One figure's answer: its picture, or the error that leaves its fenced source.
#[derive(Clone, Debug, PartialEq)]
pub struct FigureDone {
    pub ticket: u64,
    pub svg: Arc<str>,
    pub error: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Helper,
    Store,
}

enum Event {
    Started(Which, u64),
    SpawnFailed(Which, u64),
    Line(Which, u64, String),
    Exited(Which, u64, i32),
}

// A process's input and its handle; input None closes it.
struct Proc {
    generation: u64,
    input: mpsc::Sender<Option<String>>,
    child: Arc<Mutex<Option<Child>>>,
}

impl Proc {
    fn kill(&self) {
        if let Some(child) = self
            .child
            .lock()
            .ok()
            .as_mut()
            .and_then(|slot| slot.as_mut())
        {
            let _ = child.kill();
        }
    }
}

// Starts the process off the UI thread; its lines and its exit come back as events tagged with its generation.
fn launch(
    which: Which,
    generation: u64,
    argv: Vec<String>,
    events: async_channel::Sender<Event>,
) -> Proc {
    let (input, inbox) = mpsc::channel::<Option<String>>();
    let child = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&child);
    let _ = thread::Builder::new()
        .name("flea-figure-io".into())
        .spawn(move || {
            let spawned = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut process) = spawned else {
                let _ = events.send_blocking(Event::SpawnFailed(which, generation));
                return;
            };
            let (stdin, stdout) = (process.stdin.take(), process.stdout.take());
            if let Ok(mut held) = slot.lock() {
                *held = Some(process);
            }
            let _ = events.send_blocking(Event::Started(which, generation));
            let _ = thread::Builder::new()
                .name("flea-figure-in".into())
                .spawn(move || {
                    let Some(mut stdin) = stdin else { return };
                    while let Ok(Some(line)) = inbox.recv() {
                        if stdin
                            .write_all(line.as_bytes())
                            .and_then(|_| stdin.flush())
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if events
                        .send_blocking(Event::Line(which, generation, line))
                        .is_err()
                    {
                        return;
                    }
                }
            }
            let process = slot.lock().ok().and_then(|mut held| held.take());
            let code = process
                .and_then(|mut p| p.wait().ok())
                .and_then(|status| status.code())
                .unwrap_or(-1);
            let _ = events.send_blocking(Event::Exited(which, generation, code));
        });
    Proc {
        generation,
        input,
        child,
    }
}

/// The app's figure service and its two processes.
pub struct FigureHost {
    pub service: Service,
    pub binary: PathBuf,
    origin: Instant,
    helper: Option<Proc>,
    store: Option<Proc>,
    generation: u64,
    events: async_channel::Sender<Event>,
    timer: Option<(u64, Task<()>)>,
    _pump: Task<()>,
}

impl EventEmitter<FigureDone> for FigureHost {}

struct Figures(Entity<FigureHost>);

impl Global for Figures {}

/// The app's one figure host, made on first use.
pub fn host(cx: &mut App) -> Entity<FigureHost> {
    if let Some(figures) = cx.try_global::<Figures>() {
        return figures.0.clone();
    }
    let host = cx.new(FigureHost::new);
    cx.set_global(Figures(host.clone()));
    host
}

impl FigureHost {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (events, inbox) = async_channel::unbounded();
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(event) = inbox.recv().await {
                if this.update(cx, |host, cx| host.event(event, cx)).is_err() {
                    break;
                }
            }
        });
        FigureHost {
            service: Service::default(),
            binary: crate::window::startup::binary(),
            origin: cx.background_executor().now(),
            helper: None,
            store: None,
            generation: 0,
            events,
            timer: None,
            _pump: pump,
        }
    }

    /// Milliseconds on the app's clock, which a test advances by hand.
    pub fn now(&self, cx: &App) -> u64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_millis() as u64
    }

    /// Asks for one figure; its answer is the FigureDone of the ticket, emitted at once when held.
    pub fn ask(
        &mut self,
        kind: FigureKind,
        source: &str,
        display: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> u64 {
        let now = self.now(cx);
        let ticket = self.service.ask(kind, source, display, theme, now);
        self.flush(cx);
        ticket
    }

    /// The held answer for exactly this figure, so a figure typeset ahead draws in the first frame.
    pub fn cached(
        &mut self,
        kind: FigureKind,
        source: &str,
        theme: &Theme,
        display: bool,
    ) -> Option<String> {
        self.service.cached(kind, source, theme, display)
    }

    /// A document with figures is shown: the helper starts now so it is warm for the first ask.
    pub fn warm(
        &mut self,
        figures: &[(FigureKind, String)],
        themes: &dyn Fn(FigureKind) -> Option<Theme>,
        cx: &mut Context<Self>,
    ) {
        let now = self.now(cx);
        if self.service.warm(figures, themes, now) {
            self.flush(cx);
        }
    }

    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let current = |proc: &Option<Proc>, generation: u64| {
            proc.as_ref().is_some_and(|p| p.generation == generation)
        };
        match event {
            Event::Started(Which::Helper, g) if current(&self.helper, g) => {
                self.service.helper_started(now)
            }
            Event::SpawnFailed(Which::Helper, g) if current(&self.helper, g) => {
                self.service.helper_spawn_failed(now)
            }
            Event::Line(Which::Helper, g, line) if current(&self.helper, g) => {
                self.service.helper_line(&line, now)
            }
            Event::Exited(Which::Helper, g, code) if current(&self.helper, g) => {
                self.helper = None;
                self.service.helper_exited(code, now);
            }
            Event::Started(Which::Store, g) if current(&self.store, g) => {
                self.service.store_started(now)
            }
            Event::SpawnFailed(Which::Store, g) if current(&self.store, g) => {
                self.service.store_spawn_failed(now)
            }
            Event::Line(Which::Store, g, line) if current(&self.store, g) => {
                self.service.store_line(&line, now)
            }
            Event::Exited(Which::Store, g, _) if current(&self.store, g) => {
                self.store = None;
                self.service.store_exited(now);
            }
            _ => {}
        }
        self.flush(cx);
    }

    fn spawn(&mut self, which: Which, argv: Vec<String>) -> Proc {
        self.generation += 1;
        launch(which, self.generation, argv, self.events.clone())
    }

    // Carries out what the service asked for, and wakes it at its next deadline.
    fn flush(&mut self, cx: &mut Context<Self>) {
        let binary = self.binary.to_string_lossy().into_owned();
        for command in self.service.take_commands() {
            match command {
                Command::HelperSpawn(kinds) => {
                    let mut argv = vec![binary.clone(), "--figure-helper".to_string()];
                    if !kinds.is_empty() {
                        argv.push(format!("--warm={}", kinds.join(",")));
                    }
                    self.helper = Some(self.spawn(Which::Helper, argv));
                }
                Command::HelperWrite(line) => {
                    if let Some(helper) = &self.helper {
                        let _ = helper.input.send(Some(line));
                    }
                }
                Command::HelperKill | Command::HelperStop => {
                    if let Some(helper) = &self.helper {
                        helper.kill();
                    }
                }
                Command::StoreSpawn => {
                    self.store = Some(self.spawn(
                        Which::Store,
                        vec![binary.clone(), "--figure-store".to_string()],
                    ))
                }
                Command::StoreWrite(line) => {
                    if let Some(store) = &self.store {
                        let _ = store.input.send(Some(line));
                    }
                }
                Command::StoreKill => {
                    if let Some(store) = &self.store {
                        store.kill();
                    }
                }
                Command::StoreCloseInput => {
                    if let Some(store) = &self.store {
                        let _ = store.input.send(None);
                    }
                }
                Command::Done { ticket, svg, error } => cx.emit(FigureDone {
                    ticket,
                    svg: svg.into(),
                    error,
                }),
                Command::Log(_) => {}
            }
        }
        self.schedule(cx);
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.service.next_wake() else {
            self.timer = None;
            return;
        };
        if matches!(self.timer, Some((at, _)) if at == due) {
            return;
        }
        let wait = due.saturating_sub(self.now(cx));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(wait))
                .await;
            let _ = this.update(cx, |host, cx| {
                host.timer = None;
                let now = host.now(cx);
                host.service.tick(now);
                host.flush(cx);
            });
        });
        self.timer = Some((due, task));
    }
}
