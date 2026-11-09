//! The places service: the Favorites writer, its inspection and the history reader, all run off the UI thread.
use flea::i18n::I18n;
use flea::jsondoc::{self, Json};
use flea::ui_logic_rail_places::favourites::{Failure, Favourites, Operation, INSPECT_WAIT_MS};
use flea::ui_logic_rail_places::order::saved_places;
use flea::ui_logic_rail_places::recent::{bookmarks, entries, Visit};
use gpui::{Context, SharedString};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The sentence a failure says, in the active language.
pub fn say(catalog: &I18n, failure: &Failure) -> SharedString {
    let args = failure.args();
    let said = if args.is_empty() {
        catalog.tr(failure.key())
    } else {
        catalog.tr_args(failure.key(), &args)
    };
    said.into()
}

/// Reads the history file whole; a missing or unreadable file is an empty Recent.
pub fn read_history(path: &str) -> Vec<Visit> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let marks = bookmarks(&text);
    entries(
        marks
            .iter()
            .map(|(href, stamp)| (href.as_str(), stamp.as_str())),
    )
}

/// The saved favourites as `flea --ui-state` reads them back; None when the state cannot be read.
fn reread_favourites(exe: &PathBuf) -> Option<Vec<Json>> {
    let out = Command::new(exe)
        .arg("--ui-state")
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let state = jsondoc::parse(&String::from_utf8_lossy(&out.stdout)).ok()?;
    state.get("places").map(|_| saved_places(&state).favourites)
}

/// Runs the CLI with one argument, killed once wait passes, so a dead mount never holds a guard; a kill exits None.
fn run_bounded(exe: &PathBuf, wire: &str, wait: Duration) -> (Option<i32>, String) {
    let Ok(mut child) = Command::new(exe)
        .arg("--favourites")
        .arg(wire)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return (None, String::new());
    };
    let mut pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(pipe) = pipe.as_mut() {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    });
    let deadline = Instant::now() + wait;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    (code, reader.join().unwrap_or_default())
}

pub struct Places {
    /// Tags seen in accepted listings during this window's session, retained when a folder is left.
    pub tags_in_use: [bool; 7],
    pub catalog: Arc<I18n>,
    /// The flea binary `--favourites` runs through, so every caller shares the CLI's lock and re-read.
    pub exe: PathBuf,
    pub favourites: Favourites,
    /// Moves whenever a write found records another window changed, so the rows built from them re-read.
    pub external_changes: u64,
    pub inspect_wait: Duration,
    pub history: Vec<Visit>,
    pub history_reads: u64,
    /// The newest history read asked; an older answer that lands later is dropped.
    history_generation: u64,
    /// The last sentence for the status bar; none after a clean write.
    pub said: Option<SharedString>,
}

impl Places {
    pub fn new(catalog: Arc<I18n>, exe: PathBuf) -> Self {
        Self {
            tags_in_use: [false; 7],
            catalog,
            exe,
            favourites: Favourites::default(),
            external_changes: 0,
            inspect_wait: Duration::from_millis(INSPECT_WAIT_MS),
            history: Vec::new(),
            history_reads: 0,
            history_generation: 0,
            said: None,
        }
    }

    /// Seam for the G1 persisted state service: it calls this with Settings.data at startup and with each ui.json change, so another window's Favorites edit reaches this rail.
    pub fn adopt_state(&mut self, state: &Json, cx: &mut Context<Self>) {
        let records = saved_places(state).favourites;
        if records != self.favourites.records && self.favourites.records_changed(records) {
            self.external_changes += 1;
        }
        cx.notify();
    }

    /// One write at a time; the CLI runs on the background executor, and a refused write re-reads the state before it lands.
    pub fn write(&mut self, operation: &Operation, cx: &mut Context<Self>) {
        let wire = match self.favourites.apply(operation) {
            Ok(wire) => wire,
            Err(failure) => {
                self.said = Some(say(&self.catalog, &failure));
                cx.notify();
                return;
            }
        };
        let exe = self.exe.clone();
        let ran = cx.background_executor().spawn(async move {
            let (code, stdout, stderr) =
                match Command::new(&exe).arg("--favourites").arg(wire).output() {
                    Ok(out) => (
                        out.status.code(),
                        String::from_utf8_lossy(&out.stdout).into_owned(),
                        String::from_utf8_lossy(&out.stderr).into_owned(),
                    ),
                    Err(_) => (None, String::new(), String::new()),
                };
            let refreshed = if code == Some(0) {
                None
            } else {
                reread_favourites(&exe)
            };
            (code, stdout, stderr, refreshed)
        });
        cx.spawn(async move |this, cx| {
            let (code, stdout, stderr, refreshed) = ran.await;
            let _ = this.update(cx, |places, cx| {
                let done = places.favourites.finish(code, &stdout, &stderr, refreshed);
                places.external_changes += u64::from(done.external_change);
                places.said = done
                    .failure
                    .as_ref()
                    .map(|failure| say(&places.catalog, failure));
                cx.notify();
            });
        })
        .detach();
    }

    /// Checks the visible favourites that have no status yet; one runs at a time and stops at inspect_wait.
    pub fn inspect(&mut self, visible: &[usize], cx: &mut Context<Self>) {
        let Some(wire) = self.favourites.inspect(visible) else {
            return;
        };
        let (exe, wait) = (self.exe.clone(), self.inspect_wait);
        let ran = cx
            .background_executor()
            .spawn(async move { run_bounded(&exe, &wire, wait) });
        cx.spawn(async move |this, cx| {
            let (code, answer) = ran.await;
            let _ = this.update(cx, |places, cx| {
                if let Some(failure) = places.favourites.inspected(code, &answer) {
                    places.said = Some(say(&places.catalog, &failure));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-reads the history when the rail or a pane asks; the read and the parse stay off the UI thread, and only the newest lands.
    pub fn reread_history(&mut self, path: String, cx: &mut Context<Self>) {
        self.history_generation += 1;
        let generation = self.history_generation;
        let read = cx
            .background_executor()
            .spawn(async move { read_history(&path) });
        cx.spawn(async move |this, cx| {
            let visits = read.await;
            let _ = this.update(cx, |places, cx| {
                if generation != places.history_generation {
                    return;
                }
                places.history = visits;
                places.history_reads += 1;
                cx.notify();
            });
        })
        .detach();
    }
}
