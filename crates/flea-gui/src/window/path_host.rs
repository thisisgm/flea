//! The path surface's host side: the facts a commit is judged by, and what the surface's answers ask of the panes.
use super::{navigate::Step, startup::View};
use crate::{
    path::{
        failed, history,
        rank::Jump,
        view::{Inputs, Out, PathView},
        Said,
    },
    state::Shell,
};
use flea::{i18n::I18n, jsondoc::Json};
use gpui::{App, AppContext, Context, Entity, SharedString, Window};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

// The desktop's recent file, named once from the environment the window started with.
fn history_file() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| history::history_path(std::env::var("XDG_DATA_HOME").ok().as_deref(), &home()))
}

/// When the history file last changed, its modification time and length; None while it is absent.
pub(crate) type Stamp = Option<(std::time::SystemTime, u64)>;

fn stamp(file: &Path) -> Stamp {
    let meta = std::fs::metadata(file).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

// One read for a worker: the stamp first, so a change landing during the read is seen by the next check.
pub(crate) fn load(file: &Path) -> (Stamp, Vec<String>) {
    let stamp = stamp(file);
    (stamp, std::fs::read_to_string(file).map(|text| history::read(&text)).unwrap_or_default())
}

// The environment's home, read once: crumbs and completions are judged against it every frame.
fn home() -> SharedString {
    static HOME: OnceLock<SharedString> = OnceLock::new();
    HOME.get_or_init(|| std::env::var("HOME").unwrap_or_default().into()).clone()
}

/// The path surface a window hosts: the shipped jump ranker, and the favourites the ui state keeps.
pub(crate) fn path_surface(catalog: Arc<I18n>, state: &Json, cx: &mut App) -> Entity<PathView> {
    let favourites = history::favourites(state);
    cx.new(|cx| {
        let mut view = PathView::new(catalog, Box::new(Jump::default()), cx);
        view.set_favourites(favourites);
        view
    })
}

impl Shell {
    /// The recent history, read and parsed on a worker; the jump's whole ask waits for it.
    fn read_history(&mut self, cx: &mut Context<Self>) {
        let read = cx.background_executor().spawn(async move { load(history_file()) });
        self.path_waiting += 1;
        cx.spawn(async move |this, cx| {
            let (stamp, paths) = read.await;
            let _ = this.update(cx, |shell, cx| {
                shell.path_waiting -= 1;
                shell.path_history = stamp;
                shell.path.update(cx, |view, cx| view.history_read(paths, cx));
            });
        })
        .detach();
    }

    /// After each edit the file is stated on a worker, so the next open reads it again only when it changed.
    fn check_history(&mut self, cx: &mut Context<Self>) {
        let check = cx.background_executor().spawn(async move { stamp(history_file()) });
        cx.spawn(async move |this, cx| {
            let stamp = check.await;
            let _ = this.update(cx, |shell, cx| {
                if stamp != shell.path_history {
                    shell.path.update(cx, |view, _| view.history_changed());
                }
            });
        })
        .detach();
    }

    pub(crate) fn path_inputs(&self) -> Inputs {
        let dual = self.view == View::Dual;
        let side = if dual { self.focus_side.min(1) } else { 0 };
        let host = self.panes[side].as_ref();
        Inputs {
            paths: self.path_labels.clone(),
            home: home(),
            // The places producer names aliased roots; until it lands every crumb reads its own name.
            aliases: Vec::new(),
            side,
            dual,
            // The Trash view is a later producer, so no pane is showing it yet.
            failed: host.is_some_and(|host| failed(host.pane.state.kind, host.pane.busy(), host.pane.state.search, false)),
            show_hidden: host.is_some_and(|host| host.nav.folders.borrow().hidden),
            first: host.map_or(0, |host| host.pane.first_count()),
        }
    }

    pub(crate) fn sync_path(&self, cx: &mut Context<Self>) {
        let inputs = self.path_inputs();
        self.path.update(cx, |view, cx| view.sync(inputs, cx));
    }

    /// keys.toml's pathBar action from the listing.
    pub(crate) fn open_path_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_path(cx);
        self.path.update(cx, |view, cx| view.open(window, cx));
    }

    fn path_status(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(host) = self.panes[self.focus_side].as_mut() {
            host.status = text.into();
            cx.notify();
        }
    }

    pub(crate) fn path_out(&mut self, view: &Entity<PathView>, out: &Out, window: &mut Window, cx: &mut Context<Self>) {
        match out.clone() {
            Out::Open { side, path } => {
                let _ = self.step(side, Step::Open(path), window, cx);
            }
            // Opening a network address is the places producer's; until then the bar says so.
            Out::Network(_) => self.path_status(Said::NetworkUnavailable.text(&self.locale.catalog), cx),
            Out::Said(text) => self.path_status(text, cx),
            Out::Send { side, line } => {
                let Some(host) = self.panes[side].as_mut() else { return };
                match host.backend.as_ref().map(|backend| backend.line(line)) {
                    Some(Ok(())) => self.path_waiting += 1,
                    Some(Err(error)) => host.record_error(error, &self.locale),
                    None => {}
                }
            }
            Out::Focus(side) => self.focus_pane(side, cx),
            Out::ReadHistory => self.read_history(cx),
            // A closed bar hands the keyboard back, unless the press that closed it moved the keyboard elsewhere.
            Out::Closed => {
                if view.read(cx).focus.contains_focused(window, cx) || window.focused(cx).is_none() {
                    window.focus(&self.focus, cx);
                }
                self.check_history(cx);
            }
        }
    }
}
