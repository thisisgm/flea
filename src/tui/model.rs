use super::wire::{count, flag, number, text, word, Wire};
use crate::jsondoc::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Row {
    pub name: String,
    pub directory: bool,
    pub size: usize,
    pub mode: usize,
    pub link: String,
    pub kind: String,
    pub thumbnail: bool,
    pub modified: i64,
    pub icon: String,
}
impl Row {
    pub fn is_pdf(&self) -> bool {
        !self.directory && self.link.is_empty()
            && std::path::Path::new(&self.name).extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    }

    pub fn parse(row: &Json, kinds: &[Json]) -> Self {
        Self {
            name: text(row, "n").into(),
            directory: flag(row, "d"),
            size: count(row, "s"),
            mode: count(row, "p"),
            link: text(row, "l").into(),
            thumbnail: flag(row, "t"),
            modified: row.get("m").and_then(Json::as_f64).unwrap_or(0.0) as i64,
            icon: text(row, "i").into(),
            kind: kinds
                .get(count(row, "k"))
                .and_then(Json::as_str)
                .unwrap_or("")
                .into(),
        }
    }
}
#[derive(Clone)]
pub struct Tab {
    pub path: PathBuf,
    pub cursor: usize,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
}
#[derive(Default)]
pub struct Deletion {
    pub token: usize,
    pub count: usize,
    pub bytes: usize,
    pub destructive: bool,
    pub scroll: usize,
}
pub struct Model {
    pub path: PathBuf,
    pub pending: Option<PathBuf>,
    pub rows: BTreeMap<usize, Row>,
    pub parents: Vec<Row>,
    pub total: usize,
    pub cursor: usize,
    pub top: usize,
    pub height: usize,
    pub hidden: bool,
    pub sort: String,
    pub reverse: bool,
    pub folders_first: bool,
    pub group_by_kind: bool,
    pub selected: BTreeSet<usize>,
    pub selected_rows: BTreeMap<usize, Row>,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub error: String,
    pub errors: VecDeque<String>,
    pub message: String,
    pub message_at: std::time::Instant,
    pub search: String,
    pub search_query: String,
    pub searching: bool,
    pub search_from: Option<PathBuf>,
    pub search_here: bool,
    pub transfer: String,
    pub pending_clipboard: bool,
    pub clipboard: Vec<String>,
    pub cut: bool,
    pub preview: Vec<String>,
    pub preview_display: Vec<String>,
    pub preview_body: String,
    pub preview_reader: super::preview::Reader,
    pub preview_metadata: Vec<String>,
    pub preview_line_count: usize,
    pub preview_layout_scroll: usize,
    pub preview_layout_height: usize,
    pub preview_width: usize,
    pub preview_layout_generation: usize,
    pub preview_path: PathBuf,
    pub preview_visible: bool,
    pub preview_auto: bool,
    pub preview_loaded: bool,
    pub preview_focus: bool,
    pub quicklook: bool,
    pub sheet: bool,
    pub properties: Option<Vec<String>>,
    pub action_id: usize,
    pub menu: bool,
    pub menu_cursor: usize,
    pub menu_top: usize,
    pub menu_ready: bool,
    pub menu_action: String,
    pub menu_path: PathBuf,
    pub menu_directory: bool,
    pub menu_count: usize,
    pub editor: Option<super::editor::Editor>,
    pub preset: String,
    pub player: Option<super::media::Player>,
    pub pdf: Option<super::pdf::Pdf>,
    pub preview_scroll: usize,
    pub image_file: Option<PathBuf>,
    pub thumb_index: Option<usize>,
    pub filter: String,
    pub restore_cursor: Option<usize>,
    pub restore_path: Option<PathBuf>,
    pub sheet_top: usize,
    pub transfer_id: usize,
    pub transfer_total: usize,
    pub transfer_moving: bool,
    pub quit: bool,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    pub wrap: bool,
    pub navigation_before: Option<(Vec<PathBuf>, Vec<PathBuf>, usize)>,
    pub columns: usize,
    pub completion: String,
    pub completer: super::completion::Completion,
    pub preview_failed: Option<PathBuf>,
    pub playback: Option<(PathBuf, f64, bool)>,
    pub taildrop: super::taildrop::Taildrop,
    pub taildrop_target: Option<super::taildrop::Peer>,
    pub last_click: Option<(PathBuf, std::time::Instant)>,
    pub drag_anchor: Option<usize>,
    pub key_arm: String,
    pub trash_armed_at: Option<std::time::Instant>,
    pub preview_generation: usize,
    pub preview_requested: usize,
    pub preview_children: Option<PathBuf>,
    pub launches: Vec<(String, std::process::Child)>,
    pub bulk: Option<super::batch::Batch>,
    pub deletion: Option<Deletion>,
    pub restore_marks: BTreeMap<PathBuf, Row>,
    pub restore_selection: BTreeSet<PathBuf>,
    pub restore_id: usize,
    pub delete_marks: BTreeMap<PathBuf, Row>,
}
impl Model {
    pub fn new(path: PathBuf, settings: &Json) -> Self {
        let preview = settings.get("preview").unwrap_or(&Json::Null);
        Self {
            path: path.clone(),
            pending: None,
            rows: BTreeMap::new(),
            parents: Vec::new(),
            total: 0,
            cursor: 0,
            top: 0,
            height: 20,
            hidden: flag(settings, "hidden"),
            sort: settings
                .get("sort")
                .map(|s| text(s, "key"))
                .filter(|s| !s.is_empty())
                .unwrap_or("name")
                .into(),
            reverse: settings.get("sort").is_some_and(|s| flag(s, "reverse")),
            folders_first: settings
                .get("foldersFirst")
                .and_then(Json::as_bool)
                .unwrap_or(true),
            group_by_kind: flag(settings, "groupByKind"),
            selected: BTreeSet::new(),
            selected_rows: BTreeMap::new(),
            tabs: vec![Tab {
                path,
                cursor: 0,
                back: Vec::new(),
                forward: Vec::new(),
            }],
            tab: 0,
            error: String::new(),
            errors: VecDeque::new(),
            message: String::new(),
            message_at: std::time::Instant::now(),
            search: String::new(),
            search_query: String::new(),
            searching: false,
            search_from: None,
            search_here: false,
            transfer: String::new(),
            pending_clipboard: false,
            clipboard: Vec::new(),
            cut: false,
            preview: Vec::new(),
            preview_display: Vec::new(),
            preview_body: String::new(),
            preview_reader: super::preview::Reader::default(),
            preview_metadata: Vec::new(),
            preview_line_count: 0,
            preview_layout_scroll: 0,
            preview_layout_height: 0,
            preview_width: 0,
            preview_layout_generation: 0,
            preview_path: PathBuf::new(),
            preview_visible: preview
                .get("column")
                .and_then(Json::as_bool)
                .unwrap_or(true),
            preview_auto: text(preview, "loadOn") != "manual",
            preview_loaded: false,
            preview_focus: false,
            quicklook: false,
            sheet: false,
            properties: None,
            action_id: 0,
            menu: false,
            menu_cursor: 0,
            menu_top: 0,
            menu_ready: false,
            menu_action: String::new(),
            menu_path: PathBuf::new(),
            menu_directory: false,
            menu_count: 0,
            editor: None,
            preset: match text(settings, "keys") {
                "vim" => "vim",
                "mac" => "mac",
                "windows" => "windows",
                _ => "default",
            }
            .into(),
            player: None,
            pdf: None,
            preview_scroll: 0,
            image_file: None,
            thumb_index: None,
            filter: String::new(),
            restore_cursor: None,
            restore_path: None,
            sheet_top: 0,
            transfer_id: 0,
            transfer_total: 0,
            transfer_moving: false,
            quit: false,
            back: Vec::new(),
            forward: Vec::new(),
            wrap: flag(settings, "wrapAtEnds"),
            navigation_before: None,
            columns: 80,
            completion: String::new(),
            completer: super::completion::Completion::new(settings),
            preview_failed: None,
            playback: None,
            taildrop: super::taildrop::Taildrop::new(),
            taildrop_target: None,
            last_click: None,
            drag_anchor: None,
            key_arm: String::new(),
            trash_armed_at: None,
            preview_generation: 0,
            preview_requested: 0,
            preview_children: None,
            launches: Vec::new(),
            bulk: None,
            deletion: None,
            restore_marks: BTreeMap::new(),
            restore_selection: BTreeSet::new(),
            restore_id: 0,
            delete_marks: BTreeMap::new(),
        }
    }
    pub fn open(&mut self, path: PathBuf, wire: &mut Wire) -> io::Result<()> {
        if self.pending.is_some() {
            return Ok(());
        }
        if self.cancel_taildrop() {
            wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
        }
        if path == self.path && self.navigation_before.is_none() {
            self.restore_cursor.get_or_insert(self.cursor);
            if self.restore_path.is_none() && self.menu_action != "deleteRestore" { self.restore_path = self.current_path(); }
        }
        self.pending = Some(path.clone());
        let result = wire.send(vec![
            ("c", word("list")),
            ("path", word(&path.to_string_lossy())),
            ("first", number(self.height)),
            ("hidden", Json::Bool(self.hidden)),
            ("by", word(&self.sort)),
            ("desc", Json::Bool(self.reverse)),
            ("foldersFirst", Json::Bool(self.folders_first)),
            ("groupByKind", Json::Bool(self.group_by_kind)),
        ]);
        if result.is_err() {
            self.pending = None;
            self.restore_navigation();
        }
        result
    }
    pub fn restore_navigation(&mut self) {
        if let Some((back, forward, tab)) = self.navigation_before.take() {
            self.back = back;
            self.forward = forward;
            self.tab = tab;
        }
        self.restore_cursor = None;
        self.restore_path = None;
    }
    pub fn refresh(&mut self, wire: &mut Wire) -> io::Result<()> {
        if !self.search.is_empty() && !self.search_query.is_empty() {
            self.searching = true;
            self.search = "Search: refreshing".into();
            self.invalidate_rows();
            wire.send(vec![("c", word("search")), ("path", word(&self.path.to_string_lossy())),
                ("query", word(&self.search_query)), ("hidden", Json::Bool(self.hidden))])
        } else { self.open(self.path.clone(), wire) }
    }
    pub fn window(&mut self, wire: &mut Wire) -> io::Result<()> {
        if self.cursor < self.top {
            self.top = self.cursor;
        }
        if self.cursor >= self.top + self.height {
            self.top = self.cursor + 1 - self.height;
        }
        wire.send(vec![
            ("c", word("window")),
            ("start", number(self.top)),
            ("count", number(self.height)),
        ])
    }
    pub fn row_path(&self, row: &Row) -> PathBuf {
        self.path.join(&row.name)
    }
    pub fn current_path(&self) -> Option<PathBuf> {
        self.rows.get(&self.cursor).map(|row| self.row_path(row))
    }
    pub fn single_row(&self) -> Option<(usize, Row)> {
        match self.selected.len() {
            0 => self.rows.get(&self.cursor).cloned().map(|row| (self.cursor, row)),
            1 => {
                let index = *self.selected.first()?;
                self.selected_rows.get(&index).or_else(|| self.rows.get(&index)).cloned().map(|row| (index, row))
            }
            _ => None,
        }
    }
    pub fn next_rename(&mut self, wire: &mut Wire) -> io::Result<()> {
        let Some(batch) = self.bulk.take() else { return Ok(()); };
        if batch.send_next(wire, self.action_id)? {
            self.transfer = format!("Renaming {} of {}", batch.completed + 1, batch.total);
            self.menu_action = "bulkRename".into();
            self.bulk = Some(batch);
        } else {
            self.transfer.clear();
            self.menu_action.clear();
            self.say(if batch.completed == 0 { if batch.cancelled { "Rename cancelled" } else { "No names changed" }.into() }
                else { format!("{} {} items · Undo reverts one", if batch.cancelled { "Stopped after renaming" } else { "Renamed" }, batch.completed) });
            self.restore_path = batch.last;
            wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
            self.refresh(wire)?;
        }
        Ok(())
    }
    fn locate_survivors(&mut self, wire: &mut Wire) -> io::Result<()> {
        if self.menu_action != "deleteRestore" { return Ok(()); }
        self.restore_id = self.restore_id.wrapping_add(1).max(1);
        wire.send(vec![("c", word("locate")), ("id", number(self.restore_id)), ("menuId", number(self.action_id)),
            ("paths", Json::Arr(self.restore_selection.iter().map(|path| word(&path.to_string_lossy())).collect()))])
    }
    fn restore_survivors(&mut self, value: &Json) -> bool {
        if self.menu_action != "deleteRestore" || self.pending.is_some() || self.searching
            || count(value, "id") != self.restore_id || text(value, "directory") != self.path.to_string_lossy() { return false; }
        if flag(value, "ok") {
            for item in value.get("matches").and_then(Json::as_array).unwrap_or(&[]) {
                let path = PathBuf::from(text(item, "path"));
                let Some(index) = item.get("index").and_then(Json::as_f64).filter(|index| index.is_finite() && *index >= 0.0 && index.fract() == 0.0 && *index < self.total as f64) else { continue; };
                if !self.restore_selection.remove(&path) { continue; }
                self.selected.insert(index as usize);
                if let Some(row) = self.restore_marks.remove(&path) { self.selected_rows.insert(index as usize, row); }
            }
            if let Some(index) = self.selected.first() { self.cursor = *index; }
        } else { self.fail(format!("Could not restore deletion selection: {}", text(value, "error"))); }
        self.restore_selection.clear();
        self.restore_marks.clear();
        self.menu_action.clear();
        true
    }
    fn refresh_deletion(&mut self, wire: &mut Wire) -> io::Result<()> {
        self.deletion = Some(Deletion::default());
        self.delete_marks.clear();
        self.menu_action = "deleteRefresh".into();
        wire.send(vec![("c", word("menuaction")), ("op", word("refreshDelete")), ("id", number(self.action_id))])
    }
    pub fn menu_enabled(&self, index: usize) -> bool {
        if self.taildrop.submenu { return self.menu_ready && !self.taildrop.loading() && index < self.taildrop.peers.len(); }
        match index {
            0 => self.menu_ready && !self.menu_path.as_os_str().is_empty(),
            1 => true,
            2 => self.menu_ready && self.menu_count > 0 && !self.taildrop.loading() && !self.taildrop.peers.is_empty(),
            _ => false,
        }
    }
    fn cancel_taildrop(&mut self) -> bool {
        if self.taildrop_target.take().is_none() { return false; }
        self.menu_action.clear();
        self.menu = false;
        self.message.clear();
        self.fail("Taildrop cancelled; selection changed.".into());
        true
    }
    pub fn advance_taildrop(&mut self, wire: &mut Wire) -> io::Result<()> {
        if self.menu_action != "taildropRefresh" || self.taildrop.loading() { return Ok(()); }
        let current = self.taildrop_target.as_ref().and_then(|peer| self.taildrop.current_peer(peer));
        if current.is_none() || !self.taildrop.error.is_empty() {
            let reason = if self.taildrop.error.is_empty() { "Taildrop device is unreachable.".into() }
                else { self.taildrop.error.clone() };
            self.taildrop_target = None;
            self.menu_action.clear();
            self.message.clear();
            self.fail(reason);
            return wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))]);
        }
        self.taildrop_target = current;
        self.menu_action = "taildrop".into();
        wire.send(vec![("c", word("menuaction")), ("op", word("validate")), ("id", number(self.action_id)), ("action", word("taildrop"))])
    }
    pub fn say(&mut self, message: String) {
        self.message = message;
        self.message_at = std::time::Instant::now();
    }
    pub fn fail(&mut self, error: String) {
        if error.is_empty() { return; }
        if self.error.is_empty() { self.error = error; }
        else if error != self.error && !self.errors.contains(&error) { self.errors.push_back(error); }
    }
    pub fn dismiss_error(&mut self) {
        self.error = self.errors.pop_front().unwrap_or_default();
    }
    pub fn shown(&self) -> Vec<usize> {
        self.rows
            .iter()
            .filter(|(_, row)| {
                self.filter.is_empty()
                    || row
                        .name
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
            })
            .map(|(&i, _)| i)
            .collect()
    }
    pub fn apply_filter(&mut self, query: String) {
        self.filter = query;
        let shown = self.shown();
        self.selected.retain(|i| shown.contains(i));
        if !shown.contains(&self.cursor) {
            self.cursor = shown.first().copied().unwrap_or(self.top);
        }
    }
    pub fn indices(&self) -> Json {
        Json::Arr(if self.selected.is_empty() {
            vec![number(self.cursor)]
        } else {
            self.selected.iter().copied().map(number).collect()
        })
    }
    pub fn remember_selection(&mut self) {
        self.selected_rows.retain(|i, _| self.selected.contains(i));
        for (&i, row) in &self.rows {
            if self.selected.contains(&i) {
                self.selected_rows.insert(i, row.clone());
            }
        }
    }
    pub fn invalidate_rows(&mut self) {
        self.cancel_taildrop();
        self.rows.clear();
        self.selected.clear();
        self.selected_rows.clear();
        self.preview_path.clear();
        self.preview_reader.cancel();
        self.preview.clear();
        self.preview_body.clear();
        self.preview_metadata.clear();
        self.preview_display.clear();
        self.preview_loaded = false;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.preview_failed = None;
        self.preview_scroll = 0;
        self.image_file = None;
        self.thumb_index = None;
        self.player = None;
        self.pdf = None;
    }
    pub fn move_by(&mut self, delta: isize, extend: bool, wire: &mut Wire) -> io::Result<()> {
        if self.total == 0 || self.pending.is_some() {
            return Ok(());
        }
        if !self.filter.is_empty() {
            let shown = self.shown();
            if shown.is_empty() {
                return Ok(());
            }
            let index = shown.iter().position(|i| *i == self.cursor).unwrap_or(0);
            if extend {
                self.selected.insert(self.cursor);
            }
            let next = (index as isize + delta)
                .max(0)
                .min(shown.len() as isize - 1) as usize;
            self.cursor = shown[next];
            if extend {
                self.selected.insert(self.cursor);
            }
            return Ok(());
        }
        if extend {
            self.selected.insert(self.cursor);
        }
        let next = self.cursor as isize + delta;
        self.cursor = if self.wrap {
            next.rem_euclid(self.total as isize) as usize
        } else {
            next.max(0).min(self.total as isize - 1) as usize
        };
        if extend {
            self.selected.insert(self.cursor);
        }
        self.window(wire)
    }
    pub fn receive(&mut self, value: Json, wire: &mut Wire) -> io::Result<()> {
        match text(&value, "t") {
            "listed" => {
                if let Some(path) = self.pending.take() {
                    self.path = path;
                    self.cursor = self.restore_cursor.take().unwrap_or(0);
                    self.top = self.cursor;
                    self.search.clear();
                    self.search_query.clear();
                    self.searching = false;
                    self.search_from = None;
                    self.filter.clear();
                    self.tabs[self.tab].path = self.path.clone();
                    self.navigation_before = None;
                }
                self.total = count(&value, "n");
                self.invalidate_rows();
                self.cursor = self.cursor.min(self.total.saturating_sub(1));
                let parent = self
                    .path
                    .parent()
                    .unwrap_or(&self.path)
                    .to_string_lossy()
                    .into_owned();
                wire.send(vec![
                    ("c", word("peek")),
                    ("path", word(&parent)),
                    ("focus", word(&self.path.file_name().unwrap_or_default().to_string_lossy())),
                    ("first", number(self.height)),
                    ("hidden", Json::Bool(self.hidden)),
                ])?;
                self.window(wire)?;
                if let Some(path) = self.restore_path.as_ref().filter(|_| !self.searching) {
                    wire.send(vec![
                        ("c", word("locate")),
                        ("path", word(&path.to_string_lossy())),
                    ])?;
                }
                if !self.searching { self.locate_survivors(wire)?; }
            }
            "located" if value.get("matches").is_some() => {
                if self.restore_survivors(&value) {
                    self.window(wire)?;
                    wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                }
            }
            "located" => {
                if self.pending.is_none()
                    && text(&value, "directory") == self.path.to_string_lossy()
                    && self
                        .restore_path
                        .as_ref()
                        .is_some_and(|p| p.to_string_lossy() == text(&value, "path"))
                {
                    let index = value.get("index").and_then(Json::as_f64).unwrap_or(-1.0);
                    if index >= 0.0 && index < self.total as f64 {
                        self.cursor = index as usize;
                        self.window(wire)?;
                    } else {
                        if !self.search.is_empty() {
                            if let Some(parent) = self.restore_path.as_ref().and_then(|path| path.parent()).map(PathBuf::from) {
                                self.open(parent, wire)?;
                            } else { self.restore_path = None; }
                        } else { self.restore_path = None; }
                    }
                }
            }
            "rows" => {
                let empty = Vec::new();
                let kinds = value
                    .get("kinds")
                    .and_then(Json::as_array)
                    .unwrap_or(&empty);
                let rows = value.get("rows").and_then(Json::as_array).unwrap_or(&empty);
                let start = count(&value, "start");
                self.rows.clear();
                for (i, row) in rows.iter().enumerate() {
                    self.rows.insert(start + i, Row::parse(row, kinds));
                }
                self.remember_selection();
                if let Some(path) = &self.restore_path {
                    if let Some((&i, _)) = self.rows.iter().find(|(_, r)| self.row_path(r) == *path)
                    {
                        self.cursor = i;
                        self.restore_path = None;
                        if self.cursor < self.top || self.cursor >= self.top + self.height {
                            self.window(wire)?;
                        }
                    }
                }
            }
            "thumbed" => {
                if self.thumb_index == Some(count(&value, "row"))
                    && !text(&value, "file").is_empty()
                {
                    self.image_file = Some(PathBuf::from(text(&value, "file")));
                }
            }
            "meta" => {
                if self.pending.is_none()
                    && count(&value, "row") == self.cursor
                    && count(&value, "token") == self.preview_requested
                    && self.current_path().as_ref() == Some(&self.preview_path)
                {
                    for (label, value) in [
                        (
                            "Dimensions",
                            if count(&value, "w") > 0 && count(&value, "h") > 0 {
                                format!("{} × {}", count(&value, "w"), count(&value, "h"))
                            } else {
                                String::new()
                            },
                        ),
                        (
                            "Duration",
                            if count(&value, "ms") > 0 {
                                super::media::clock(count(&value, "ms") as f64 / 1000.0)
                            } else {
                                String::new()
                            },
                        ),
                        (
                            "Rate",
                            if count(&value, "rate") > 0 {
                                format!("{} kHz", count(&value, "rate") as f64 / 1000.0)
                            } else {
                                String::new()
                            },
                        ),
                        (
                            "Entries",
                            if count(&value, "entries") > 0 {
                                count(&value, "entries").to_string()
                            } else {
                                String::new()
                            },
                        ),
                        (
                            "Unpacked",
                            if count(&value, "unpacked") > 0 {
                                super::render::bytes(count(&value, "unpacked"))
                            } else {
                                String::new()
                            },
                        ),
                    ] {
                        if !value.is_empty() {
                            self.preview_metadata.push(format!("{} · {}", label, value));
                        }
                    }
                    if flag(&value, "afailed") {
                        self.preview_metadata
                            .push("This archive could not be read.".into());
                    }
                    self.preview_width = 0;
                }
            }
            "peeked" => {
                if self
                    .preview_children
                    .as_ref()
                    .is_some_and(|path| path.to_string_lossy() == text(&value, "path"))
                    && self.current_path().as_ref() == self.preview_children.as_ref()
                {
                    self.preview.truncate(2);
                    if flag(&value, "failed") {
                        self.preview.push("This folder could not be read.".into());
                    } else {
                        for row in value.get("rows").and_then(Json::as_array).unwrap_or(&[]) {
                            self.preview.push(format!(
                                "{} {}",
                                if flag(row, "d") { "›" } else { "□" },
                                text(row, "n")
                            ));
                        }
                        if count(&value, "n") == 0 {
                            self.preview.push("Empty folder".into());
                        }
                    }
                    self.preview_width = 0;
                    self.preview_children = None;
                    return Ok(());
                }
                if self
                    .path
                    .parent()
                    .map(|p| p.to_string_lossy() == text(&value, "path"))
                    .unwrap_or(false)
                {
                    self.parents = value
                        .get("rows")
                        .and_then(Json::as_array)
                        .unwrap_or(&[])
                        .iter()
                        .map(|r| Row::parse(r, &[]))
                        .collect();
                }
            }
            "changed" => {
                if self.deletion.is_some() && self.menu_action == "deleteConfirm" {
                    self.refresh_deletion(wire)?;
                }
                if self.pending.is_none()
                    && self.search.is_empty()
                    && text(&value, "path") == self.path.to_string_lossy()
                {
                    self.restore_cursor = Some(self.cursor);
                    if self.menu_action != "deleteRestore" { self.restore_path = self.current_path(); }
                    self.open(self.path.clone(), wire)?;
                }
            }
            "paths" => {
                let paths: Vec<String> = value
                    .get("paths")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect();
                if self.pending_clipboard {
                    self.pending_clipboard = false;
                    self.clipboard = paths;
                    self.say(format!(
                        "{} items {}",
                        self.clipboard.len(),
                        if self.cut { "cut" } else { "copied" }
                    ));
                }
            }
            "searching" | "searched" => {
                if self.search.is_empty() || self.pending.is_some() {
                    return Ok(());
                }
                self.total = count(&value, "n");
                self.searching = text(&value, "t") == "searching";
                self.search = format!(
                    "Search: {} matches · {} scanned",
                    self.total,
                    count(&value, "scanned")
                );
                if !self.searching {
                    // Ranking changes every index; no action may use the discovery-order window.
                    self.invalidate_rows();
                    self.cursor = 0;
                    self.top = 0;
                    if let Some(path) = &self.restore_path {
                        wire.send(vec![("c", word("locate")), ("path", word(&path.to_string_lossy()))])?;
                    }
                    self.locate_survivors(wire)?;
                }
                self.cursor = self.cursor.min(self.total.saturating_sub(1));
                self.window(wire)?;
            }
            "transferstarted" => {
                self.transfer_id = count(&value, "id");
                self.transfer_total = count(&value, "n");
                self.transfer_moving = flag(&value, "moving");
                self.transfer = format!(
                    "{} 0 of {}",
                    if self.transfer_moving {
                        "Moving"
                    } else {
                        "Copying"
                    },
                    self.transfer_total
                );
            }
            "transferprogress" => {
                if count(&value, "id") == self.transfer_id {
                    self.transfer = format!(
                        "{} {} of {}",
                        if self.transfer_moving {
                            "Moving"
                        } else {
                            "Copying"
                        },
                        count(&value, "index") + 1,
                        self.transfer_total
                    );
                }
            }
            "transferitem" => {
                if !flag(&value, "ok") {
                    self.fail(format!("{}: {}", text(&value, "name"), text(&value, "err")));
                }
            }
            "renamed" if self.bulk.is_some() && self.menu_action == "bulkRename" => {
                let result = if flag(&value, "ok") { self.bulk.as_mut().unwrap().complete(text(&value, "path")) }
                    else { Err(io::Error::other("Bulk rename stopped: the pending rename failed")) };
                if let Err(error) = result {
                    self.fail(error.to_string());
                    self.bulk.as_mut().unwrap().cancelled = true;
                }
                self.next_rename(wire)?;
            }
            "transferdone" | "trashed" | "undone" | "redone" | "renamed" | "made"
            | "duplicated" => {
                let operation = text(&value, "t");
                if operation == "transferdone" {
                    if count(&value, "id") != self.transfer_id {
                        return Ok(());
                    }
                    self.transfer.clear();
                    self.transfer_id = 0;
                }
                if matches!(
                    operation,
                    "undone" | "redone" | "renamed" | "made" | "duplicated"
                ) && !flag(&value, "ok")
                {
                    if let Some(editor) = &mut self.editor {
                        editor.pending = false;
                    }
                    return Ok(());
                }
                self.say(match operation {
                    "renamed" => "Renamed · Undo available".into(),
                    "made" => "Folder created · Undo available".into(),
                    "duplicated" => "Duplicated · Undo available".into(),
                    "undone" => format!("Undid {}", text(&value, "op")),
                    "redone" => format!("Redid {} · Undo available", text(&value, "op")),
                    "trashed" => format!(
                        "Moved {} items to Trash · Undo available",
                        count(&value, "ok")
                    ),
                    _ => format!(
                        "{} {} items{}",
                        if flag(&value, "cancelled") {
                            "Cancelled after"
                        } else {
                            "Transferred"
                        },
                        count(&value, "ok"),
                        if count(&value, "skipped") > 0 {
                            format!(" · {} skipped", count(&value, "skipped"))
                        } else {
                            String::new()
                        }
                    ),
                });
                if count(&value, "failed") > 0 {
                    self.fail(format!("{} failed", count(&value, "failed")));
                }
                if matches!(operation, "renamed" | "made" | "duplicated") {
                    self.restore_path = Some(PathBuf::from(text(&value, "path")));
                    self.editor = None;
                }
                self.refresh(wire)?;
            }
            "menuaction" if text(&value, "op") == "newFile" => {
                if count(&value, "id") != self.action_id || !self
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.kind == "newfile" && e.pending)
                {
                    return Ok(());
                }
                if flag(&value, "ok") {
                    self.editor = None;
                    self.say("File created · Undo available".into());
                    self.restore_path = Some(PathBuf::from(text(&value, "path")));
                    self.refresh(wire)?;
                } else if let Some(editor) = &mut self.editor {
                    editor.pending = false;
                    editor.error = text(&value, "error").into();
                }
            }
            "menuaction" if text(&value, "op") == "delete" && count(&value, "id") == self.action_id => {
                if !matches!(self.menu_action.as_str(), "deleteRunning" | "deleteCancelling") { return Ok(()); }
                if flag(&value, "stale") && self.menu_action == "deleteRunning" {
                    self.say("Selection changed; review the new confirmation".into());
                    self.refresh_deletion(wire)?;
                    return Ok(());
                }
                self.menu_action.clear();
                self.deletion = None;
                if !flag(&value, "ok") {
                    if flag(&value, "cancelled") { self.say("Deletion cancelled".into()); }
                    else { self.fail(text(&value, "error").into()); }
                } else if flag(&value, "stale") {
                    self.say("Deletion cancelled".into());
                } else {
                    self.say(format!("Deleted {} of {}{}", count(&value, "deleted"), self.menu_count,
                        if flag(&value, "cancelled") { " · Cancelled".into() } else { String::new() }));
                    if count(&value, "failed") > 0 { self.fail(format!("{} failed · {}", count(&value, "failed"), text(&value, "error"))); }
                    let remaining: BTreeSet<PathBuf> = value.get("remaining").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(PathBuf::from).collect();
                    self.restore_marks = std::mem::take(&mut self.delete_marks);
                    self.restore_marks.retain(|path, _| remaining.contains(path));
                    self.restore_path = None;
                    self.restore_selection = remaining;
                    if !self.restore_selection.is_empty() { self.menu_action = "deleteRestore".into(); }
                    self.refresh(wire)?;
                }
                if self.menu_action != "deleteRestore" {
                    self.delete_marks.clear();
                    wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                }
            }
            "menuaction" if !self.menu_action.is_empty() && count(&value, "id") == self.action_id => {
                if !flag(&value, "ok") {
                    if !flag(&value, "cancelled") { self.fail(text(&value, "error").into()); }
                    self.menu_action.clear();
                    self.menu = false;
                    if self.taildrop_target.take().is_some() { self.message.clear(); }
                    self.deletion = None;
                    wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                } else if text(&value, "op") == "snapshot" && self.menu_action == "menu" {
                    self.menu_ready = true;
                } else if text(&value, "op") == "snapshot" && self.menu_action == "deleteSnapshot" {
                    self.menu_action = "deletePrepare".into();
                    wire.send(vec![("c", word("menuaction")), ("op", word("prepareDelete")), ("id", number(self.action_id))])?;
                } else if (text(&value, "op") == "prepareDelete" && self.menu_action == "deletePrepare")
                    || (text(&value, "op") == "refreshDelete" && self.menu_action == "deleteRefresh") {
                    let token = count(&value, "token");
                    if token == 0 { return Err(io::Error::other("Delete review returned no confirmation token")); }
                    self.menu_count = count(&value, "count");
                    self.deletion = Some(Deletion { token, count: self.menu_count, bytes: count(&value, "bytes"), destructive: false, scroll: 0 });
                    self.menu_action = "deleteConfirm".into();
                } else if text(&value, "op") == "snapshot" && self.menu_action == "bulkSnapshot" {
                    self.menu_action = "bulk".into();
                    wire.send(vec![("c", word("menuaction")), ("op", word("validate")), ("id", number(self.action_id)), ("action", word("bulk"))])?;
                } else if text(&value, "op") == "validate" && text(&value, "action") == self.menu_action {
                    let paths: Vec<String> = value.get("paths").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(str::to_owned).collect();
                    let action = std::mem::take(&mut self.menu_action);
                    if action == "bulk" {
                        if paths.len() != self.menu_count {
                            self.fail("Bulk rename selection is incomplete; select the items again".into());
                        } else {
                            match super::batch::Batch::new(paths.into_iter().map(PathBuf::from).collect()) {
                                Ok(batch) => { self.bulk = Some(batch); self.menu_action = "bulkEditor".into(); }
                                Err(error) => self.fail(error.to_string()),
                            }
                        }
                        if self.menu_action != "bulkEditor" {
                            wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                        }
                    } else if action == "open" {
                        if !paths.iter().any(|path| PathBuf::from(path) == self.menu_path) {
                            self.fail("Open failed: the validated selection omitted the selected path".into());
                        } else if self.menu_directory {
                            if self.pending.is_some() { self.menu_action = "openWaiting".into(); }
                            else {
                                let path = self.menu_path.clone();
                                super::actions::navigate(self, path, wire)?;
                            }
                        } else if crate::open::open(&self.menu_path.to_string_lossy()) != 0 {
                            self.fail("Could not open selected file".into());
                        }
                    } else if action == "taildrop" {
                        self.message.clear();
                        if let Some(peer) = self.taildrop_target.take() {
                            if self.menu_count == 0 || paths.len() < self.menu_count {
                                self.fail("Taildrop: selection is incomplete.".into());
                            } else {
                                match self.taildrop.send(&peer, &paths[..self.menu_count]) {
                                    Ok(()) => self.say(format!("Sending to {}", peer.label)),
                                    Err(error) => self.fail(format!("Taildrop: {}", crate::error::io_message(&error))),
                                }
                            }
                        }
                        wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                    }
                }
            }
            "menuaction" if self.sheet && self.properties.is_some() && count(&value, "id") == self.action_id => {
                if !flag(&value, "ok") {
                    self.properties = Some(vec![text(&value, "error").into()]);
                } else if text(&value, "op") == "snapshot" {
                    wire.send(vec![("c", word("menuaction")), ("op", word("properties")), ("id", number(self.action_id))])?;
                } else if text(&value, "op") == "properties" {
                    let mut facts = vec![
                        text(&value, "path").into(),
                        format!("Kind · {}", text(&value, "kind")),
                        format!("Size · {}", super::render::bytes(count(&value, "bytes"))),
                        format!("Modified · {}", super::render::modified(value.get("modified").and_then(Json::as_f64).unwrap_or(0.0) as i64)),
                        format!("Permissions · {}", text(&value, "mode")),
                        format!("Owner · {} ({})", text(&value, "owner"), count(&value, "uid")),
                        format!("Group · {}", count(&value, "gid")),
                    ];
                    if flag(&value, "symlink") { facts.push(format!("Target · {}", text(&value, "target"))); }
                    self.properties = Some(facts);
                }
            }
            "error" => {
                self.fail(format!("{}: {}", text(&value, "where"), text(&value, "msg")));
                if text(&value, "where") == "rename" && self.bulk.is_some() {
                    self.bulk.as_mut().unwrap().cancelled = true;
                    self.next_rename(wire)?;
                }
                if let Some(editor) = &mut self.editor {
                    if text(&value, "where") == editor.kind {
                        editor.pending = false;
                        editor.error = text(&value, "msg").into();
                    }
                }
                if text(&value, "where") == "scan"
                    && self
                        .pending
                        .as_ref()
                        .is_some_and(|p| p.to_string_lossy() == text(&value, "path"))
                {
                    self.pending = None;
                    self.restore_navigation();
                }
                if self.menu_action == "deleteRestore" && matches!(text(&value, "where"), "scan" | "search") {
                    self.menu_action.clear();
                    self.restore_selection.clear();
                    self.restore_marks.clear();
                    wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                }
                if text(&value, "where") == "paths" {
                    self.pending_clipboard = false;
                    self.taildrop_target = None;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listing_invalidation_cancels_pending_taildrop_intent() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        model.taildrop_target = Some(super::super::taildrop::Peer { id: "node".into(), label: "peer".into(), address: "peer.invalid".into() });
        model.menu_action = "taildropRefresh".into();
        model.message = "Checking Taildrop".into();
        model.invalidate_rows();
        assert!(model.taildrop_target.is_none());
        assert!(model.menu_action.is_empty());
        assert!(model.message.is_empty());
        assert_eq!(model.error, "Taildrop cancelled; selection changed.");
        assert!(!model.cancel_taildrop(), "late invalidation must not create another error");
    }


    #[test]
    fn pdf_preview_uses_filename_not_generic_icon_or_localized_kind() {
        let mut row = Row::parse(&Json::Obj(vec![
            ("n".into(), word("pages.PDF")),
            ("i".into(), word("x-office-document")),
        ]), &[word("Document")]);
        assert!(row.is_pdf());
        row.directory = true;
        assert!(!row.is_pdf());
        row.directory = false;
        row.link = "original.pdf".into();
        assert!(!row.is_pdf());
        row.link.clear();
        for name in [".pdf", "pages.pdf.txt", "report.doc", "no-extension"] {
            row.name = name.into();
            assert!(!row.is_pdf(), "{name}");
        }
    }

    #[test]
    fn survivor_restore_requires_current_reply_and_original_membership() {
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.menu_action = "deleteRestore".into();
        model.restore_id = 7;
        model.total = 100;
        model.restore_selection = [PathBuf::from("/listing/survivor"), PathBuf::from("/listing/replaced")].into_iter().collect();
        let reply = |id, directory| Json::Obj(vec![("id".into(), number(id)), ("directory".into(), word(directory)),
            ("ok".into(), Json::Bool(true)), ("matches".into(), Json::Arr(vec![
                Json::Obj(vec![("path".into(), word("/listing/survivor")), ("index".into(), number(75))]),
                Json::Obj(vec![("path".into(), word("/listing/unselected")), ("index".into(), number(3))])]))]);
        assert!(!model.restore_survivors(&reply(6, "/listing")));
        assert!(!model.restore_survivors(&reply(7, "/other")));
        model.pending = Some(PathBuf::from("/listing"));
        assert!(!model.restore_survivors(&reply(7, "/listing")));
        model.pending = None;
        assert!(model.restore_survivors(&reply(7, "/listing")));
        assert_eq!(model.selected, [75].into_iter().collect());
        assert_eq!(model.cursor, 75);
        assert!(model.selected_rows.is_empty(), "offscreen survivors remain selected without invented metadata");
        assert!(model.restore_selection.is_empty());
        assert!(model.menu_action.is_empty());
    }

    #[test]
    fn one_mark_keeps_its_identity_when_cursor_leaves_the_page() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        let row = |name| Row::parse(&Json::Obj(vec![("n".into(), word(name))]), &[]);
        model.rows.insert(3, row("marked"));
        model.selected.insert(3);
        model.remember_selection();
        model.rows.clear();
        model.rows.insert(8, row("cursor"));
        model.cursor = 8;
        let (index, selected) = model.single_row().unwrap();
        assert_eq!(index, 3);
        assert_eq!(selected.name, "marked");
        model.selected.insert(8);
        assert!(model.single_row().is_none());
        model.selected.clear();
        assert_eq!(model.single_row().unwrap().1.name, "cursor");
    }

    #[test]
    fn errors_survive_until_each_is_acknowledged() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        model.fail("Copy failed".into());
        model.fail("Preview failed".into());
        model.fail("Copy failed".into());
        model.fail("Preview failed".into());
        model.fail(String::new());
        assert_eq!(model.error, "Copy failed");
        assert_eq!(model.errors.len(), 1);
        model.dismiss_error();
        assert_eq!(model.error, "Preview failed");
        model.dismiss_error();
        assert!(model.error.is_empty());
    }

    #[test]
    fn menu_requires_a_ready_identity_snapshot() {
        let mut model = Model::new(PathBuf::from("/"), &Json::Null);
        model.menu_path = PathBuf::from("/selected");
        model.menu_count = 1;
        assert!(!model.menu_enabled(0));
        assert!(model.menu_enabled(1));
        model.menu_ready = true;
        assert!(model.menu_enabled(0));
        assert!(!model.menu_enabled(2));
        model.menu_path.clear();
        assert!(!model.menu_enabled(0));
    }
}
