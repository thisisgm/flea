use super::wire::{count, flag, number, text, word, Wire};
use crate::jsondoc::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::path::PathBuf;

// Matches StatusBar.messageMs in ui/StatusBar.qml.
const MESSAGE_TIME: std::time::Duration = std::time::Duration::from_secs(4);
const FOLDER_SIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(120);

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
    pub destructive: bool,
    pub scroll: usize,
}
pub struct Model {
    pub font_marks: bool,
    pub force_text_marks: bool,
    pub disk_path: PathBuf,
    fs_requested: Option<PathBuf>,
    pub disk_fs: String,
    pub disk_free: usize,
    pub disk_class: String,
    pub folder_sizes: BTreeMap<usize, (usize, bool)>,
    size_pending: BTreeSet<usize>,
    size_view: (PathBuf, usize, usize, String),
    size_at: std::time::Instant,
    pub empty_at: std::time::Instant,
    pub scrolled_at: Option<std::time::Instant>,
    pub preview_rows: Vec<Row>,
    pub preview_rows_generation: usize,
    pub preview_refusal: String,
    pub text_lines: Vec<(Option<usize>, String)>,
    pub text_gutter: usize,
    pub last_directory: Option<PathBuf>,
    pub choosing: bool,
    pub choose_pending: bool,
    pub choose_count: usize,
    pub chosen: Option<Vec<String>>,
    pub no_cwd: bool,
    pub edit_file: Option<PathBuf>,
    pub capabilities: super::capabilities::Capabilities,
    pub path: PathBuf,
    pub pending: Option<PathBuf>,
    // The loop keeps the previous frame, so the rows under the cursor are not drawn yet.
    pub held: bool,
    pub rows: BTreeMap<usize, Row>,
    pub parents: Vec<Row>,
    peek_sent: Option<PathBuf>,
    pub total: usize,
    pub cursor: usize,
    pub top: usize,
    pub height: usize,
    pub hidden: bool,
    pub sort: String,
    pub detail: super::detail::Detail,
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
    pub find: super::contents::State,
    pub search: String,
    pub search_query: String,
    pub searching: bool,
    pub search_from: Option<PathBuf>,
    pub transfer: String,
    pub pending_clipboard: bool,
    // Keep the untagged paths request pending until its cancelled reply drains.
    pub discard_clipboard: bool,
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
    pub preview_body_bytes: usize,
    pub preview_width: usize,
    pub preview_layout_generation: usize,
    pub preview_path: PathBuf,
    pub preview_visible: bool,
    pub preview_auto: bool,
    pub preview_loaded: bool,
    pub preview_focus: bool,
    pub quicklook: bool,
    pub sheet: bool,
    pub properties: Option<Vec<(String, String)>>,
    pub action_id: usize,
    pub menu: bool,
    pub menu_cursor: usize,
    pub menu_top: usize,
    pub menu_ready: bool,
    pub menu_action: String,
    pub menu_path: PathBuf,
    pub menu_directory: bool,
    pub menu_count: usize,
    pub menu_view: super::render::menu_layout::View,
    // Whether Hyprland rounds its windows, so a card or menu frame rounds its corners to match.
    pub rounded: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub i18n: crate::i18n::I18n,
    pub editor: Option<super::editor::Editor>,
    pub preset: String,
    pub hints: super::keymap::Hints,
    pub player: Option<super::media::Player>,
    pub pdf: Option<super::pdf::Pdf>,
    pub model3d: super::model3d::State,
    pub model3d_palette: flea::ui_logic_pv4::color::Palette,
    pub preview_scroll: usize,
    pub image_file: Option<PathBuf>,
    // The cursor row's probed width, height and length in ms, once its meta answer lands.
    pub media_probe: Option<(usize, usize, usize)>,
    pub thumb_index: Option<usize>,
    pub filter: String,
    pub name_filter: super::filter::Matches,
    pub restore_cursor: Option<usize>,
    pub restore_path: Option<PathBuf>,
    // Set by a re-sort press, kept across its replies, spent by a cursor move, a new listing request or a -1 answer.
    pub sort_anchor: Option<PathBuf>,
    // Where settings persist; None in tests, so no test writes the user's state.
    pub store: Option<crate::uistore::Store>,
    pub sheet_top: usize,
    pub transfer_id: usize,
    pub job_status: super::jobstatus::Status,
    pub preview_state: super::pane_state::State,
    pub transfer_total: usize,
    pub progress_percent: Option<usize>,
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
    pub prompts: super::prompts::Prompts,
}
// "Renamed 1 item · Undo reverts one", the bulk completion line next_rename reports.
fn bulk_done_line(completed: usize, cancelled: bool) -> String {
    format!("{} {} · Undo reverts one", if cancelled { "Stopped after renaming" } else { "Renamed" }, super::render::items(completed))
}
impl Model {
    pub fn new(path: PathBuf, settings: &Json) -> Self {
        let preview = settings.get("preview").unwrap_or(&Json::Null);
        Self {
            font_marks: true,
            force_text_marks: false,
            disk_path: PathBuf::new(),
            fs_requested: None,
            disk_fs: String::new(),
            disk_free: 0,
            disk_class: String::new(),
            folder_sizes: BTreeMap::new(),
            size_pending: BTreeSet::new(),
            size_view: (PathBuf::new(), 0, 0, String::new()),
            size_at: std::time::Instant::now(),
            empty_at: std::time::Instant::now(),
            scrolled_at: None,
            preview_rows: Vec::new(),
            preview_rows_generation: usize::MAX,
            preview_refusal: String::new(),
            text_lines: Vec::new(),
            text_gutter: 0,
            last_directory: None,
            choosing: false,
            choose_pending: false,
            choose_count: 0,
            chosen: None,
            no_cwd: false,
            edit_file: None,
            capabilities: super::capabilities::Capabilities::default(),
            path: path.clone(),
            pending: None,
            held: false,
            rows: BTreeMap::new(),
            parents: Vec::new(),
            peek_sent: None,
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
            detail: super::detail::Detail::default(),
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
            find: Default::default(),
            search: String::new(),
            search_query: String::new(),
            searching: false,
            search_from: None,
            transfer: String::new(),
            pending_clipboard: false,
            discard_clipboard: false,
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
            preview_body_bytes: 0,
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
            menu_view: super::render::menu_layout::View::new(settings),
            rounded: Default::default(),
            i18n: Default::default(),
            editor: None,
            preset: match text(settings, "keys") {
                "vim" => "vim",
                "mac" => "mac",
                "windows" => "windows",
                _ => "default",
            }
            .into(),
            hints: super::keymap::Hints::default(),
            player: None,
            pdf: None,
            model3d: Default::default(),
            model3d_palette: super::graphics::palette(&super::theme::Theme::with_depth("", super::theme::Depth::Truecolor)),
            preview_scroll: 0,
            image_file: None,
            media_probe: None,
            thumb_index: None,
            filter: String::new(),
            name_filter: Default::default(),
            restore_cursor: None,
            restore_path: None,
            sort_anchor: None,
            store: None,
            sheet_top: 0,
            transfer_id: 0,
            job_status: super::jobstatus::Status::default(),
            preview_state: Default::default(),
            transfer_total: 0,
            progress_percent: None,
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
            prompts: Default::default(),
        }
    }
    pub fn open(&mut self, path: PathBuf, wire: &mut Wire) -> io::Result<()> {
        if self.pending.is_some() {
            return Ok(());
        }
        // Cleared on send: a late re-sort reply belongs to the old listing, and a re-sort sent after this names its own anchor.
        super::contents::navigate(self, wire)?;
        self.model3d.stop(wire)?;
        self.sort_anchor = None;
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
            ("foldersByName", Json::Bool(self.sort == "size")),
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
    pub fn marks(&self) -> bool {
        !self.force_text_marks && (self.font_marks || self.capabilities.graphics == super::capabilities::Protocol::Kitty)
    }
    pub fn row_size(&self, index: usize, row: &Row) -> String {
        let (bytes, partial) = self.folder_sizes.get(&index).copied().unwrap_or((row.size, row.directory));
        format!("{}{}", if partial { ">" } else { "" }, super::render::bytes(bytes))
    }
    pub fn request_sizes(&mut self, wire: &mut Wire, now: std::time::Instant) -> io::Result<()> {
        if self.pending.is_none() && self.search.is_empty() && self.fs_requested.as_ref() != Some(&self.path) {
            wire.send(vec![("c", word("fsinfo"))])?;
            self.fs_requested = Some(self.path.clone());
        }
        let view = (self.path.clone(), self.top, self.height, self.filter.clone());
        if self.pending.is_some() || self.searching || view != self.size_view {
            if !self.size_pending.is_empty() { wire.send(vec![("c", word("dirsizecancel"))])?; }
            self.size_pending.clear();
            self.size_view = view;
            self.size_at = now;
            let visible = self.visible();
            self.folder_sizes.retain(|index, _| visible.contains(index));
        }
        if self.pending.is_some() || self.searching || self.prompts.trash.is_some() || now.saturating_duration_since(self.size_at) < FOLDER_SIZE_SETTLE
            || self.disk_path != self.path || crate::backend::extclass::refuses_folder_sizes(&self.disk_class) { return Ok(()); }
        let rows: Vec<_> = self.visible().into_iter().filter(|index| self.rows.get(index).is_some_and(|row| row.directory)
            && !self.folder_sizes.contains_key(index) && !self.size_pending.contains(index)).collect();
        if rows.is_empty() { return Ok(()); }
        wire.send(vec![("c", word("dirsize")), ("rows", Json::Arr(rows.iter().map(|row| number(*row)).collect()))])?;
        self.size_pending.extend(rows);
        Ok(())
    }
    pub(super) fn resize_body(&mut self, cells: usize) {
        self.height = cells.max(1);
    }
    pub(super) fn file_capacity(&self) -> usize {
        (self.height / super::contents::row_height(self)).max(1)
    }
    pub(super) fn search_count(&self, contents: bool) -> usize {
        (self.height / if contents { 2 } else { 1 }).clamp(1, 4096)
    }
    pub(super) fn search_path(&mut self, path: PathBuf, wire: &mut Wire) -> io::Result<()> {
        if self.path == path { return Ok(()); }
        self.path = path;
        self.parents.clear();
        self.peek_sent = self.path.parent().map(PathBuf::from);
        if let Some(parent) = &self.peek_sent {
            wire.send(vec![("c", word("peek")), ("path", word(&parent.to_string_lossy())),
                ("focus", word(&self.path.file_name().unwrap_or_default().to_string_lossy())),
                ("first", number(self.height)), ("hidden", Json::Bool(self.hidden))])?;
        }
        Ok(())
    }
    pub(super) fn search_settled(&mut self, wire: &mut Wire) -> io::Result<()> {
        if let Some(path) = &self.restore_path {
            wire.send(vec![("c", word("locate")), ("path", word(&path.to_string_lossy()))])?;
        }
        self.locate_survivors(wire)
    }
    pub fn visible(&self) -> Vec<usize> {
        if self.filter.is_empty() { self.rows.range(self.top..self.top.saturating_add(self.file_capacity())).map(|(i, _)| *i).collect() }
        else { super::filter::visible(self) }
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
        if let Some(active) = &self.find.active {
            self.find.mode = active.contents;
            self.find.session.scope = active.scope;
            return super::contents::start(
                self,
                self.search_query.clone(),
                self.search_from.clone().unwrap_or_else(|| self.path.clone()),
                wire,
            )
            .map(|_| ());
        }
        if !self.search.is_empty() && !self.search_query.is_empty() {
            self.searching = true;
            self.search = self.prompts.i18n.tr("find.starting");
            self.invalidate_rows();
            self.sort_anchor = None;
            wire.send(vec![("c", word("search")), ("path", word(&self.path.to_string_lossy())),
                ("query", word(&self.search_query)), ("hidden", Json::Bool(self.hidden))])
        } else { self.open(self.path.clone(), wire) }
    }
    pub fn window(&mut self, wire: &mut Wire) -> io::Result<()> {
        if !self.filter.is_empty() { return super::filter::window(self, wire); }
        if self.prompts.trash.is_some() { return super::trashview::window(self, wire); }
        let old_top = self.top;
        let capacity = self.file_capacity();
        if self.cursor < self.top {
            self.top = self.cursor;
        }
        if self.cursor >= self.top + self.file_capacity() {
            self.top = self.cursor + 1 - capacity;
        }
        if self.top != old_top { self.scrolled_at = Some(std::time::Instant::now()); }
        wire.send(vec![
            ("c", word("window")),
            ("start", number(self.top)),
            ("count", number(capacity)),
        ])
    }
    pub fn row_path(&self, row: &Row) -> PathBuf {
        self.path.join(&row.name)
    }
    // The displayed safe identity also feeds previews; actions separately refuse a held Find snapshot.
    pub fn current_path(&self) -> Option<PathBuf> {
        self.rows.get(&self.cursor).filter(|_| !super::trashview::hides(self) && super::contents::displayed_operable(self, self.cursor)).map(|row| self.row_path(row))
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
            self.transfer = format!("Renaming {} of {}", super::render::grouped(batch.completed + 1), super::render::grouped(batch.total));
            self.menu_action = "bulkRename".into();
            self.bulk = Some(batch);
        } else {
            self.transfer.clear();
            self.menu_action.clear();
            self.say(if batch.completed == 0 { if batch.cancelled { "Rename cancelled" } else { "No names changed" }.into() }
                else { bulk_done_line(batch.completed, batch.cancelled) });
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
    fn cancel_taildrop(&mut self) -> bool {
        if self.taildrop_target.take().is_none() { return false; }
        self.menu_action.clear();
        self.menu = false;
        self.message.clear();
        self.fail("Taildrop cancelled; selection changed".into());
        true
    }
    pub fn advance_taildrop(&mut self, wire: &mut Wire) -> io::Result<()> {
        if self.menu_action != "taildropRefresh" || self.taildrop.loading() { return Ok(()); }
        let current = self.taildrop_target.as_ref().and_then(|peer| self.taildrop.current_peer(peer));
        if current.is_none() || !self.taildrop.error.is_empty() {
            let reason = if self.taildrop.error.is_empty() { "Taildrop device is unreachable".into() }
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
    pub fn expire_message(&mut self, now: std::time::Instant) {
        if !matches!(self.menu_action.as_str(), "taildropRefresh" | "taildrop")
            && !super::prompts::lasting(self)
            && now.saturating_duration_since(self.message_at) >= MESSAGE_TIME {
            self.message.clear();
        }
    }
    pub fn fail(&mut self, error: String) {
        if error.is_empty() { return; }
        if self.error.is_empty() { self.error = error; }
        else if error != self.error && !self.errors.contains(&error) { self.errors.push_back(error); }
    }
    pub fn dismiss_error(&mut self) {
        self.error = self.errors.pop_front().unwrap_or_default();
    }
    pub fn shown(&self) -> Vec<usize> { super::filter::shown(self) }
    pub fn filter_count(&self) -> usize { super::filter::count_shown(self) }
    pub fn apply_filter(&mut self, query: String, wire: &mut Wire) -> io::Result<()> { super::filter::change(self, query, wire) }
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
        self.name_filter.reset();
        self.cancel_taildrop();
        self.rows.clear();
        self.detail.invalidate();
        self.folder_sizes.clear();
        self.size_pending.clear();
        self.size_view.0.clear();
        self.selected.clear();
        self.selected_rows.clear();
        self.preview_path.clear();
        self.preview_reader.cancel();
        self.preview.clear();
        self.preview_body.clear();
        self.preview_metadata.clear();
        self.media_probe = None;
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
            return self.window(wire);
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
    // True while this listing's parent peek is still unanswered.
    pub fn peek_out(&self) -> bool {
        matches!((&self.peek_sent, self.path.parent()), (Some(sent), Some(parent)) if sent.as_path() == parent)
    }
    pub fn active_operation(&self) -> usize {
        if self.job_status.id > 0 {
            self.job_status.id
        } else {
            self.transfer_id
        }
    }
    pub fn receive(&mut self, value: Json, wire: &mut Wire) -> io::Result<()> {
        if super::contents::receive(self, &value, wire)? { return Ok(()); }
        match text(&value, "t") {
            "jobs" | "job" => super::jobstatus::receive(self, &value),
            "code" => super::syntax::receive(self, &value),
            "model3d" => super::model3d::receive(self, &value),
            "officepdf" => super::office::receive(self, &value, wire)?,
            "filtered" => super::filter::receive(self, &value, wire)?,
            "listed" => {
                wire.listing = value.get("listing").map(|_| count(&value, "listing"));
                if let Some(path) = self.pending.take() {
                    self.empty_at = std::time::Instant::now();
                    let same = self.prompts.trash.take().is_none() & (std::mem::replace(&mut self.path, path) == self.path);
                    self.last_directory = Some(self.path.clone());
                    self.cursor = self.restore_cursor.take().unwrap_or(0);
                    self.top = if same { self.top } else { self.cursor };
                    self.search.clear();
                    self.search_query.clear();
                    self.searching = false;
                    self.search_from = None;
                    self.filter.clear();
                    self.tabs[self.tab].path = self.path.clone();
                    self.navigation_before = None;
                }
                if self.total > 0 && count(&value, "n") == 0 { self.empty_at = std::time::Instant::now(); }
                self.total = count(&value, "n");
                self.invalidate_rows();
                self.cursor = self.cursor.min(self.total.saturating_sub(1));
                // Only the anchor still waited for moves the cursor; a superseded or foreign reply leaves the wait alive.
                if value.get("anchor").and_then(Json::as_str).is_some()
                    && text(&value, "path") == self.path.to_string_lossy()
                    && self.sort_anchor.as_ref().is_some_and(|waiting| waiting.to_string_lossy() == text(&value, "anchor"))
                {
                    let index = value.get("anchorIndex").and_then(Json::as_f64).unwrap_or(-1.0);
                    if index >= 0.0 && index.fract() == 0.0 && index < self.total as f64 {
                        self.cursor = index as usize;
                    } else {
                        // -1: the row is gone or hidden, so the anchor is spent and the clamped cursor stands.
                        self.sort_anchor = None;
                    }
                }
                // No parent column at /: clear it and send no peek, so nothing waits.
                match self.path.parent().map(PathBuf::from) {
                    None => {
                        self.parents.clear();
                        self.peek_sent = None;
                    }
                    Some(parent) => {
                        wire.send(vec![
                            ("c", word("peek")),
                            ("path", word(&parent.to_string_lossy())),
                            ("focus", word(&self.path.file_name().unwrap_or_default().to_string_lossy())),
                            ("first", number(self.height)),
                            ("hidden", Json::Bool(self.hidden)),
                        ])?;
                        self.peek_sent = Some(parent);
                    }
                }
                self.fs_requested = None;
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
            "rows" if self.prompts.trash.is_none() => {
                let empty = Vec::new();
                let kinds = value
                    .get("kinds")
                    .and_then(Json::as_array)
                    .unwrap_or(&empty);
                let rows = value.get("rows").and_then(Json::as_array).unwrap_or(&empty);
                let start = count(&value, "start");
                // An answered filter keeps the drawn matches other windows brought, and only those.
                let drawn = super::filter::answered(self).map(|_| self.visible());
                let kept = |i: &usize| drawn.as_ref().is_some_and(|drawn| drawn.contains(i));
                self.rows.retain(|i, _| kept(i));
                self.detail.invalidate();
                for (i, row) in rows.iter().enumerate() {
                    if drawn.is_none() || kept(&(start + i)) { self.rows.insert(start + i, Row::parse(row, kinds)); }
                }
                self.remember_selection();
                if let Some(path) = &self.restore_path {
                    if let Some((&i, _)) = self.rows.iter().find(|(_, r)| self.row_path(r) == *path)
                    {
                        self.cursor = i;
                        self.restore_path = None;
                        if self.cursor < self.top || self.cursor >= self.top + self.file_capacity() {
                            self.window(wire)?;
                        }
                    }
                }
            }
            "fsinfo" if self.pending.is_none() && text(&value, "path") == self.path.to_string_lossy() => {
                self.disk_path = self.path.clone();
                self.disk_fs = text(&value, "fs").into();
                self.disk_free = count(&value, "free");
                self.disk_class = text(&value, "class").into();
            }
            "dirsized" if self.pending.is_none() => {
                let row = count(&value, "row");
                if self.size_pending.remove(&row) && self.rows.get(&row).is_some_and(|entry| entry.directory) {
                    self.folder_sizes.insert(row, (count(&value, "bytes"), flag(&value, "partial")));
                }
            }
            "thumbed" => {
                if self.thumb_index == Some(count(&value, "row"))
                    && !text(&value, "file").is_empty()
                {
                    self.image_file = Some(PathBuf::from(text(&value, "file")));
                }
            }
            "owner" => self.detail.receive(&value, &self.rows),
            "meta" => {
                if self.pending.is_none()
                    && count(&value, "row") == self.cursor
                    && count(&value, "token") == self.preview_requested
                    && self.current_path().as_ref() == Some(&self.preview_path)
                {
                    self.media_probe = Some((count(&value, "w"), count(&value, "h"), count(&value, "ms")));
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
                    ] {
                        if !value.is_empty() {
                            self.preview_metadata.push(format!("{} · {}", label, value));
                        }
                    }
                    super::pane_state::meta(self, &value);
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
                    self.preview_rows_generation = self.preview_generation;
                    self.preview_refusal = if !flag(&value, "failed") { String::new() }
                        else if count(&value, "mode") > 0 && count(&value, "mode") & 0o444 == 0 { "You don’t have permission to open this folder.".into() }
                        else { "This folder could not be read.".into() };
                    self.preview_rows = value.get("rows").and_then(Json::as_array).unwrap_or(&[]).iter().take(self.height).map(|row| Row::parse(row, &[])).collect();
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
                    self.peek_sent = None;
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
                    && !self.choose_pending
                    && self.search.is_empty() && self.prompts.trash.is_none()
                    && text(&value, "path") == self.path.to_string_lossy()
                {
                    self.restore_cursor = Some(self.cursor);
                    if self.menu_action != "deleteRestore" { self.restore_path = self.current_path(); }
                    self.open(self.path.clone(), wire)?;
                }
            }
            "paths" => {
                if std::mem::take(&mut self.discard_clipboard) {
                    self.pending_clipboard = false;
                    return Ok(());
                }
                let paths: Vec<String> = value
                    .get("paths")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect();
                if self.choose_pending {
                    self.choose_pending = false;
                    if paths.len() != self.choose_count || paths.iter().any(|p| !std::path::Path::new(p).is_absolute() || p.contains(['\n', '\r'])) {
                        self.fail("The selection changed or cannot be written one path per line; choose it again".into());
                    } else {
                        self.chosen = Some(paths.clone());
                        self.quit = true;
                    }
                }
                if self.pending_clipboard {
                    self.pending_clipboard = false;
                    self.clipboard = paths;
                    self.say(super::prompts::clipboard(self));
                }
            }
            "searching" | "searched" => {
                if self.search.is_empty() || self.pending.is_some() {
                    return Ok(());
                }
                self.total = count(&value, "n");
                self.searching = text(&value, "t") == "searching";
                self.search = super::prompts::found(self, count(&value, "scanned"));
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
                self.progress_percent = None;
                self.transfer_id = count(&value, "id");
                self.transfer_total = count(&value, "n");
                self.transfer_moving = flag(&value, "moving");
                if self.job_status.id == 0 {
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
            }
            "transferprogress" => {
                if self.job_status.id == 0 && count(&value, "id") == self.transfer_id {
                    let total = count(&value, "total");
                    self.progress_percent = (self.transfer_total > 0 && total > 0).then(|| {
                        let done = count(&value, "index") as u128 * total as u128 + count(&value, "bytes").min(total) as u128;
                        super::jobstatus::ratio(done, total as u128 * self.transfer_total as u128)
                    });
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
                    else { Err(io::Error::other("Bulk rename stopped: a rename could not finish")) };
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
                    if self.job_status.id == 0 {
                        self.transfer.clear();
                    }
                    self.transfer_id = 0;
                    self.progress_percent = None;
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
                let line = super::prompts::receipt(self, &value);
                self.say(line);
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
                    super::prompts::record(self, "newfile", vec![PathBuf::from(text(&value, "path"))]);
                    self.say(super::prompts::undoable(self, self.prompts.i18n.tr("receipt.fileCreated")));
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
                    self.say("Selection changed; confirm again".into());
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
                    self.say(format!("Deleted {} of {} {}{}", super::render::grouped(count(&value, "deleted")), super::render::grouped(self.menu_count),
                        if self.menu_count == 1 { "item" } else { "items" },
                        if flag(&value, "cancelled") { " · Cancelled".into() } else { String::new() }));
                    if count(&value, "failed") > 0 { self.fail(format!("{} failed · {}", super::render::grouped(count(&value, "failed")), text(&value, "error"))); }
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
            "formats" | "actions" => super::render::menu_run::receive(self, &value),
            "menuaction" if !self.menu_action.is_empty() && count(&value, "id") == self.action_id => {
                if super::render::menu_run::reply(self, &value, wire)? {
                } else if !flag(&value, "ok") {
                    if !flag(&value, "cancelled") { self.fail(text(&value, "error").into()); }
                    self.menu_action.clear();
                    self.menu = false;
                    if self.taildrop_target.take().is_some() { self.message.clear(); }
                    self.deletion = None;
                    wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                } else if text(&value, "op") == "snapshot" && self.menu_action == "menu" {
                    super::render::menu_run::ready(self, wire)?;
                } else if text(&value, "op") == "snapshot" && self.menu_action == "deleteSnapshot" {
                    self.menu_action = "deletePrepare".into();
                    wire.send(vec![("c", word("menuaction")), ("op", word("prepareDelete")), ("id", number(self.action_id))])?;
                } else if (text(&value, "op") == "prepareDelete" && self.menu_action == "deletePrepare")
                    || (text(&value, "op") == "refreshDelete" && self.menu_action == "deleteRefresh") {
                    let token = count(&value, "token");
                    if token == 0 { return Err(io::Error::other("Deletion could not be confirmed; try again")); }
                    self.menu_count = count(&value, "count");
                    self.deletion = Some(Deletion { token, count: self.menu_count, destructive: false, scroll: 0 });
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
                            self.fail("Could not open selected file".into());
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
                                self.fail("Taildrop: selection is incomplete".into());
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
                    self.properties = Some(vec![(String::new(), text(&value, "error").into())]);
                } else if text(&value, "op") == "snapshot" {
                    wire.send(vec![("c", word("menuaction")), ("op", word("properties")), ("id", number(self.action_id))])?;
                } else if text(&value, "op") == "properties" {
                    self.properties = Some(super::render::cards::properties(&value, &self.i18n));
                }
            }
            "trashbrowse" => super::trashview::receive(self, &value),
            "collisions" | "fatnames" => super::prompts::collided(self, &value, wire)?,
            "error" => {
                let held = self.editor.as_mut().filter(|editor| editor.kind == text(&value, "where")).map(|editor| { editor.pending = false; editor.error = text(&value, "msg").into(); }).is_some();
                if !held { self.fail(if text(&value, "where") == "scan" {
                    if text(&value, "msg") == "permission denied" { "You don’t have permission to open this folder.".into() }
                    else { text(&value, "msg").into() }
                }
                    else { format!("{}: {}", text(&value, "where"), text(&value, "msg")) }); }
                if text(&value, "where") == "rename" && self.bulk.is_some() {
                    self.bulk.as_mut().unwrap().cancelled = true;
                    self.next_rename(wire)?;
                }
                if text(&value, "where") == "scan"
                    && self
                        .pending
                        .as_ref()
                        .is_some_and(|p| p.to_string_lossy() == text(&value, "path"))
                {
                    self.pending = None;
                    self.restore_navigation();
                    if self.find.active.is_some() { self.refresh(wire)?; }
                }
                if self.menu_action == "deleteRestore" && matches!(text(&value, "where"), "scan" | "search") {
                    self.menu_action.clear();
                    self.restore_selection.clear();
                    self.restore_marks.clear();
                    wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(self.action_id))])?;
                }
                if text(&value, "where") == "paths" {
                    self.pending_clipboard = false;
                    self.discard_clipboard = false;
                    self.choose_pending = false;
                    self.taildrop_target = None;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
