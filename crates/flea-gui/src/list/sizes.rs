//! The settled List's visible folder sizes, using the existing backend walker and bounded planner.
use super::{columns::Column, pane::Held};
use crate::{
    navigation::sort::{wants_sizes, DirSizes, Size},
    pane::requests::RowAction,
    state::Shell,
    window::startup::View,
};
use flea::jsondoc::Json;
use gpui::{Context, Task, Window};
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::Duration,
};

// ui/Pane.qml: the first rest is 70 ms, subsequent rests 120 ms, with 240 answered rows.
const FIRST_MS: u64 = 70;
const SETTLE_MS: u64 = 120;
const CAP: usize = 240;

#[derive(Debug)]
pub enum Reply {
    Filesystem { path: String, class: String },
    Sized { row: usize, size: Size },
}

pub(crate) fn parse(document: &Json) -> Result<Option<Reply>, String> {
    let text = |key| {
        document
            .get(key)
            .and_then(Json::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("{key} must be a string"))
    };
    let unsigned = |key| match document.get(key) {
        Some(Json::Num(value)) => value
            .parse::<u64>()
            .map_err(|_| format!("{key} must be an unsigned integer")),
        _ => Err(format!("{key} must be an unsigned integer")),
    };
    Ok(match document.get("t").and_then(Json::as_str) {
        Some("fsinfo") => Some(Reply::Filesystem {
            path: text("path")?,
            class: text("class")?,
        }),
        Some("dirsized") => Some(Reply::Sized {
            row: usize::try_from(unsigned("row")?).map_err(|_| "row must fit an index")?,
            size: Size {
                bytes: unsigned("bytes")?,
                partial: document
                    .get("partial")
                    .and_then(Json::as_bool)
                    .ok_or("partial must be a boolean")?,
            },
        }),
        _ => None,
    })
}

#[derive(Clone, PartialEq)]
struct Key {
    listing: usize,
    scroll: u32,
    width: u32,
    height: u32,
    typeface: super::Typeface,
    row: u32,
    held: usize,
    count: usize,
    identity: usize,
    switched: super::columns::Switched,
    stored: super::columns::StoredWidths,
    strip: u32,
    dual: bool,
    drawn: Option<View>,
}

impl Key {
    fn read(
        host: &crate::window::pane::PaneHost,
        pane: crate::window::composition::Rect,
        dual: bool,
        strip: f32,
        settings: &Json,
    ) -> Self {
        let held = &host.pane.held;
        let list = host.list.borrow();
        Self {
            listing: held.listing,
            scroll: host.pane.scroll_y.to_bits(),
            width: pane.width.to_bits(),
            height: pane.height.to_bits(),
            typeface: list.typeface.clone(),
            row: list.file_metrics().row.to_bits(),
            held: held.start,
            count: held.rows.len(),
            identity: Rc::as_ptr(held) as usize,
            switched: super::columns::Switched::from_settings(settings),
            stored: super::columns::StoredWidths::from_settings(settings),
            strip: strip.to_bits(),
            dual,
            drawn: host.drawn_view,
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    held: Rc<Held>,
    path: String,
    class: Option<String>,
    asked_fs: bool,
    asked_sizes: bool,
    sizes: DirSizes,
    history: HashMap<String, HashMap<FolderKey, Size>>,
    history_order: VecDeque<String>,
    waiting: Vec<usize>,
    key: Option<Key>,
    task: Option<Task<()>>,
    /// Backend.qml dirSizeRequests: every non-empty dirsize line this pane's backend was sent.
    pub requests: usize,
    /// Every dirsizecancel line the backend took, so a scroll that drops waiting walks can be counted (GPUI-only dirSizeCancels).
    pub cancels: usize,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct FolderKey {
    name: String,
    size: u64,
    mtime: i64,
    mode: u32,
}

impl FolderKey {
    fn of(row: &flea::protocol::RowFacts) -> Option<Self> {
        row.dir.then(|| Self {
            name: row.name.clone(),
            size: row.size,
            mtime: row.mtime,
            mode: row.mode,
        })
    }
}

#[cfg(test)]
impl State {
    /// A held listing with one answered folder walk, as a dirsized reply leaves it.
    pub(crate) fn walked_fixture(held: Rc<Held>, row: usize, size: Size) -> Self {
        let mut sizes = DirSizes::default();
        sizes.remember(row, size, CAP);
        Self {
            held,
            sizes,
            ..Self::default()
        }
    }

    /// A ready operation fixture retains the listing and its already-answered filesystem class.
    pub(crate) fn settled_fixture(held: Rc<Held>, path: String, class: &str) -> Self {
        Self {
            held,
            path,
            class: Some(class.into()),
            asked_fs: true,
            ..Self::default()
        }
    }
}

impl State {
    /// A folder's finished walk for one row of the held listing; a partial or missing walk has no size to state.
    pub(crate) fn walked(&self, listing: usize, row: usize) -> Option<u64> {
        self.sizes
            .size_for(row)
            .filter(|size| self.held.listing == listing && !size.partial)
            .map(|size| size.bytes)
    }

    /// The pane's asked and answered folder walks, which the dirSizeState reader writes out.
    pub(crate) fn dir_sizes(&self) -> &DirSizes {
        &self.sizes
    }

    /// The shown folder's filesystem class word, once the backend has answered for it.
    pub(crate) fn class(&self) -> Option<&str> {
        self.class.as_deref()
    }

    fn remembered(&self, path: &str, row: &flea::protocol::RowFacts) -> Option<Size> {
        self.history.get(path)?.get(&FolderKey::of(row)?).copied()
    }

    fn remember(&mut self, path: &str, row: &flea::protocol::RowFacts, size: Size) {
        if size.partial {
            return;
        }
        let Some(key) = FolderKey::of(row) else {
            return;
        };
        if !self.history.contains_key(path) {
            self.history_order.push_back(path.to_owned());
        } else {
            self.history_order.retain(|known| known != path);
            self.history_order.push_back(path.to_owned());
        }
        self.history
            .entry(path.to_owned())
            .or_default()
            .insert(key, size);
        while self.history_order.len() > 32 {
            if let Some(oldest) = self.history_order.pop_front() {
                self.history.remove(&oldest);
            }
        }
    }

    pub(crate) fn cancel(&mut self, backend: &crate::backend::Backend) {
        self.task = None;
        self.key = None;
        if !self.waiting.is_empty() {
            if backend.list_details(r#"{"c":"dirsizecancel"}"#).is_ok() {
                self.cancels += 1;
            }
            self.waiting.clear();
            self.sizes.cancelled();
        }
    }
}

impl Shell {
    pub(crate) fn sync_list_sizes(&mut self, side: usize, window: &Window, cx: &mut Context<Self>) {
        let geometry = self.geometry(window);
        let visible = matches!(self.view, View::List | View::Dual | View::Columns)
            && (side == 0 || self.view == View::Dual);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(backend) = &host.backend else { return };
        if !visible || host.stopped || host.pane.busy() || host.pane.state.search {
            host.sizes.cancel(backend);
            return;
        }
        let key = Key::read(
            host,
            geometry.panes[side],
            self.view == View::Dual,
            self.slots.strip_height,
            &self.launch.settings.data,
        );
        let current = &host.pane.held;
        let state = &mut host.sizes;
        // Every new answer, a re-list's first or a later window on the same listing, installs its own inline sizes.
        let fresh = !Rc::ptr_eq(&state.held, current);
        if state.held.listing != current.listing {
            state.cancel(backend);
            let carried = if state.path == host.path {
                state.sizes.carried(
                    &state.held.rows,
                    state.held.start,
                    &current.rows,
                    current.start,
                )
            } else {
                DirSizes::default()
            };
            // Keep completed sizes for unchanged folders in this path; changed directory facts need a new walk.
            state.sizes = carried;
            if state.path != host.path {
                state.class = None;
            }
            state.path = host.path.clone();
            state.asked_fs = false;
            state.asked_sizes = false;
        }
        if fresh {
            state.sizes.inlined(&current.rows, current.start, CAP);
            for (at, row) in current.rows.iter().enumerate() {
                let index = current.start + at;
                let size = state
                    .sizes
                    .size_for(index)
                    .or_else(|| state.remembered(&host.path, row));
                if let Some(size) = size {
                    state.sizes.remember(index, size, CAP);
                    state.remember(&host.path, row, size);
                }
            }
            state.held = Rc::clone(current);
            let mut list = host.list.borrow_mut();
            for (at, _) in current.rows.iter().enumerate() {
                let row = current.start + at;
                if let Some(size) = state.sizes.size_for(row) {
                    list.view.folder_size(current.listing, row, size);
                }
            }
            list.retain_folder_sizes(&state.sizes);
        }
        if !state.asked_fs {
            if let Err(error) = backend.list_details(r#"{"c":"fsinfo"}"#) {
                host.record_error(error, &self.locale);
                return;
            }
            state.asked_fs = true;
        }
        if state.key.as_ref() == Some(&key) {
            return;
        }
        state.cancel(backend);
        state.key = Some(key.clone());
        let delay = if state.asked_sizes {
            SETTLE_MS
        } else {
            FIRST_MS
        };
        state.task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| shell.ask_list_sizes(side, key, window, cx))
            });
        }));
    }

    fn ask_list_sizes(&mut self, side: usize, key: Key, window: &Window, cx: &mut Context<Self>) {
        if !matches!(self.view, View::List | View::Dual | View::Columns)
            || (side == 1 && self.view != View::Dual)
        {
            return;
        }
        let geometry = self.geometry(window);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(backend) = &host.backend else { return };
        let current = Key::read(
            host,
            geometry.panes[side],
            self.view == View::Dual,
            self.slots.strip_height,
            &self.launch.settings.data,
        );
        if host.sizes.key.as_ref() != Some(&key)
            || host.pane.busy()
            || host.stopped
            || host.pane.state.search
            || key != current
        {
            return;
        }
        host.sizes.task = None;
        let columns = self.view == View::Columns;
        let class = host.sizes.class.as_deref();
        let surface = if columns { "columns" } else { "list" };
        if !wants_sizes(surface, &[], class.unwrap_or_default(), class.is_some()) {
            return;
        }
        let held = &host.pane.held;
        let ask = if columns {
            let Some((first, last)) = crate::columns::walks::painted_rows(host, key.listing) else {
                return;
            };
            host.sizes.sizes.plan(&held.rows, held.start, first, last)
        } else {
            let list = host.list.borrow();
            if !host.drawn_list
                || list.drawn.listing != key.listing
                || !list.drawn.layout.shown(Column::Size)
            {
                return;
            }
            // A filter's listing indexes need not be adjacent or ascending.
            list.visible()
                .iter()
                .flat_map(|row| {
                    host.sizes
                        .sizes
                        .plan(&held.rows, held.start, row.index, row.index)
                })
                .collect()
        };
        if ask.is_empty() {
            return;
        }
        match backend.rows(
            "dirsize",
            RowAction {
                listing: held.listing,
                rows: ask.clone(),
            },
        ) {
            Ok(()) => {
                host.sizes.requests += 1;
                host.sizes.asked_sizes = true;
                host.sizes.sizes.applied(&ask);
                host.sizes.waiting = ask;
            }
            Err(error) => {
                host.record_error(error, &self.locale);
                cx.notify();
            }
        }
    }

    pub(crate) fn list_details(&mut self, side: usize, reply: Reply, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.pane.busy() || host.stopped {
            return;
        }
        match reply {
            Reply::Filesystem { path, class } if path == host.path => {
                if host.sizes.class.as_ref() != Some(&class) {
                    host.sizes.class = Some(class);
                    host.sizes.key = None;
                    cx.notify();
                }
            }
            Reply::Sized { row, size } if host.sizes.held.listing == host.pane.held.listing => {
                let Some(at) = host.sizes.waiting.iter().position(|asked| *asked == row) else {
                    return;
                };
                host.sizes.waiting.swap_remove(at);
                host.sizes.sizes.remember(row, size, CAP);
                if let Some(facts) = host.pane.held.row(row) {
                    host.sizes.remember(&host.path, facts, size);
                }
                let mut list = host.list.borrow_mut();
                list.view.folder_size(host.pane.held.listing, row, size);
                list.retain_folder_sizes(&host.sizes.sizes);
                cx.notify();
            }
            _ => {}
        }
    }
}

impl super::ListView {
    /// A folder's walked size for one row; a new listing forgets the last one's.
    pub fn folder_size(&mut self, listing: usize, row: usize, size: Size) {
        if listing != self.walked_listing {
            self.walked.clear();
            self.walked_listing = listing;
        }
        if self.walked.get(&row) == Some(&size) {
            return;
        }
        self.walked.insert(row, size);
        self.walked_revision += 1;
    }

    /// What a cached row's clip mark and walked size were resolved against in this listing.
    pub fn mark_revision(&self, listing: usize) -> crate::row::cache::Revision {
        (self.clip.revision, self.walked_revision, listing)
    }

    pub fn walked_for(&self, listing: usize, row: usize) -> Option<Size> {
        (listing == self.walked_listing)
            .then(|| self.walked.get(&row).copied())
            .flatten()
    }
}

#[cfg(test)]
#[path = "sizes_window_tests.rs"]
mod window_tests;
