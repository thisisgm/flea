//! Listing facts; filters run backend-side, replies name generations.
use crate::list::pane::Held;
use flea::i18n::I18n;
use flea::json::{escape, field_str, field_usize, field_usize_array};
use flea::protocol::{parse_reply, Reply, RowFacts};
use flea::ui_logic_picker_listing::{self as core, FilterDef};
use std::rc::Rc;

// The sr2 seam: one predicate over one drawn name.
pub trait NameMatcher {
    fn keeps(&mut self, name: &str) -> bool;
}

// Stand-in matcher until sr2 joins: the query as a case-folded substring.
pub struct SubstringMatcher {
    query: String,
}

impl SubstringMatcher {
    pub fn of(query: &str) -> Self {
        Self {
            query: query.to_owned(),
        }
    }
}

impl NameMatcher for SubstringMatcher {
    fn keeps(&mut self, name: &str) -> bool {
        self.query.is_empty() || name.to_lowercase().contains(&self.query.to_lowercase())
    }
}

// Why a listing stopped, each with its own sentence through core::list_failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    // Scan/sort errors carry the backend's msg as reason.
    Refused(String),
    // The backend died: its held rows stay, the footer says so.
    Gone,
    // The worker never started, so the restart sentence applies.
    Unstarted,
    // An unparsable reply fails, never reads as empty.
    Protocol,
}

impl Cause {
    fn state(&self) -> &'static str {
        match self {
            Cause::Gone => "dead",
            Cause::Unstarted => "unstarted",
            Cause::Refused(_) | Cause::Protocol => "failed",
        }
    }
}

// The backend filter answer: whole-list total, matching rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filtered {
    pub query: String,
    pub id: usize,
    pub total: usize,
    pub rows: Vec<usize>,
}

// The name query awaiting its answer; the id must match.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PendingFilter {
    id: usize,
}

// Escape clears a set name filter first, then cancels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escape {
    ClearedFilter,
    Cancel,
}

// What pk1 copies into its ListingFacts each time the listing lands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub location: String,
    pub total: usize,
    pub first: usize,
    pub held: usize,
    pub names: Vec<String>,
    pub cursor: usize,
    pub state: &'static str,
    pub failed: bool,
    pub cursor_file: Option<String>,
    pub chip: i64,
    // The last non-failing error the footer says.
    pub notice: String,
    // The backend filter's answer the board's shown count draws from.
    pub filtered: Option<Filtered>,
}

#[derive(Default)]
pub struct Listing {
    pub(super) location: String,
    generation: u64,
    pending: bool,
    cause: Option<Cause>,
    said: String,
    held: Rc<Held>,
    cursor: usize,
    pub(super) filter: Option<FilterDef>,
    pub(super) chip: i64,
    pub(super) show_hidden: bool,
    pub(super) folders: Option<crate::navigation::folder_state::Folders>,
    filter_seq: usize,
    pending_filter: Option<PendingFilter>,
    filtered: Option<Filtered>,
}

impl Listing {
    pub fn open(&mut self, path: &str) {
        self.location = path.to_owned();
        self.generation += 1;
        self.pending = true;
        self.cause = None;
        self.said.clear();
        self.cursor = 0;
        self.pending_filter = None;
        self.filtered = None;
        self.held = Rc::new(Held {
            listing: self.generation as usize,
            ..Held::default()
        });
    }

    pub fn location(&self) -> &str {
        &self.location
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn held(&self) -> Rc<Held> {
        self.held.clone()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    // Recent's request: the history's paths, newest first, listed under the base "/".
    pub fn paths_line(&self, paths: &[String], first: usize) -> String {
        let paths: Vec<String> = paths
            .iter()
            .map(|path| format!("\"{}\"", escape(path)))
            .collect();
        format!(
            r#"{{"c":"listpaths","paths":[{}],"first":{first},{}}}"#,
            paths.join(","),
            core::list_fields(self.filter.as_ref(), self.show_hidden)
        )
    }

    // The window request for rows the held window does not hold yet.
    pub fn window_request(&self, start: usize, count: usize) -> Option<String> {
        if self.pending || self.cause.is_some() || self.total() == 0 {
            return None;
        }
        Some(format!(
            r#"{{"c":"window","start":{start},"count":{count}}}"#
        ))
    }

    // Reads one reply line; an unparsable line fails.
    pub fn receive(&mut self, generation: u64, line: &str) -> bool {
        match parse_reply(line) {
            Ok(Reply::Listed { total, .. }) => self.landed(generation, total),
            Ok(Reply::Rows { start, rows, .. }) => self.held_rows(generation, start, rows),
            Ok(Reply::Error {
                operation, message, ..
            }) => self.worker_error(generation, &operation, &message),
            Ok(Reply::Other) => self.filtered_answer(generation, line),
            Err(_) => self.fail(generation, Cause::Protocol),
        }
    }

    // PickerWindow.qml:496-503: scan/sort fail; dead backend keeps rows.
    fn worker_error(&mut self, generation: u64, operation: &str, message: &str) -> bool {
        if generation != self.generation {
            return false;
        }
        match operation {
            "scan" | "sort" => self.fail(generation, Cause::Refused(message.to_owned())),
            "backend" => self.fail(generation, Cause::Gone),
            _ => {
                self.said = message.to_owned();
                true
            }
        }
    }

    // An empty query clears at once; no new request goes out.
    pub fn filter_line(&mut self, query: &str) -> String {
        if query.is_empty() {
            self.filtered = None;
            self.pending_filter = None;
            return String::new();
        }
        self.filter_seq += 1;
        let id = self.filter_seq;
        self.pending_filter = Some(PendingFilter { id });
        format!(r#"{{"c":"filter","query":"{}","id":{id}}}"#, escape(query))
    }

    // A filtered answer lands on its own id; cursor to first match.
    fn filtered_answer(&mut self, generation: u64, line: &str) -> bool {
        if field_str(line, "t").as_deref() != Some("filtered") {
            return false;
        }
        if generation != self.generation || self.pending || self.cause.is_some() {
            return false;
        }
        let query = field_str(line, "query").unwrap_or_default();
        // An empty query or null rows clears the filter entirely.
        if query.is_empty() || line.contains("\"rows\":null") {
            self.filtered = None;
            self.pending_filter = None;
            return true;
        }
        let asked = match &self.pending_filter {
            Some(asked) => *asked,
            None => return false,
        };
        let id = field_usize(line, "id").unwrap_or(usize::MAX);
        if id != asked.id {
            return false;
        }
        let rows = field_usize_array(line, "rows");
        let total = field_usize(line, "n").unwrap_or(self.total());
        if let Some(first) = rows.first() {
            self.cursor = *first;
        }
        self.filtered = Some(Filtered {
            query,
            id,
            total,
            rows,
        });
        self.pending_filter = None;
        true
    }

    // Count is the backend total after caller/hidden filters.
    pub fn landed(&mut self, generation: u64, total: usize) -> bool {
        if generation != self.generation || !self.pending {
            return false;
        }
        self.pending = false;
        self.held = Rc::new(Held {
            listing: self.generation as usize,
            total,
            ..Held::default()
        });
        true
    }

    pub fn held_rows(&mut self, generation: u64, start: usize, rows: Vec<RowFacts>) -> bool {
        let total = self.total();
        if generation != self.generation
            || self.pending
            || self.cause.is_some()
            || start.checked_add(rows.len()).is_none_or(|end| end > total)
        {
            return false;
        }
        self.held = Rc::new(Held {
            listing: self.generation as usize,
            total,
            start,
            rows,
        });
        true
    }

    // A current-generation failure counts; a landed listing keeps rows.
    pub fn fail(&mut self, generation: u64, cause: Cause) -> bool {
        if generation != self.generation {
            return false;
        }
        self.pending = false;
        self.cause = Some(cause);
        true
    }

    // Cancelling bumps the generation; the late answer refuses.
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.pending = false;
        self.pending_filter = None;
    }

    // The listing failure line; scan/sort carry backend msg as reason.
    pub fn failure_line(&self, i18n: &I18n) -> String {
        match &self.cause {
            Some(Cause::Refused(msg)) => core::list_failed_reason(i18n, msg),
            Some(cause) => core::list_failed(i18n, cause.state()),
            None => String::new(),
        }
    }

    // A deep clear drops held rows; the window re-asks.
    pub fn deep_clear(&mut self) {
        self.held = Rc::new(Held {
            listing: self.generation as usize,
            total: self.total(),
            ..Held::default()
        });
    }

    pub fn total(&self) -> usize {
        self.held.total
    }

    // Escape clears a set filter first, else cancels.
    pub fn escape(&mut self) -> Escape {
        if self.filtered.is_some() || self.pending_filter.is_some() {
            self.filtered = None;
            self.pending_filter = None;
            Escape::ClearedFilter
        } else {
            Escape::Cancel
        }
    }

    // Type-ahead over held rows only; the board find is the backend filter.
    pub fn hunt(&mut self, matcher: &mut dyn NameMatcher) -> Option<usize> {
        let found = self
            .held
            .rows
            .iter()
            .enumerate()
            .find(|(_, row)| matcher.keeps(&row.name))
            .map(|(at, _)| self.held.start + at)?;
        self.cursor = found;
        Some(found)
    }

    pub fn move_cursor(&mut self, to: usize) {
        if self.total() > 0 {
            self.cursor = to.min(self.total() - 1);
        }
    }

    pub fn state(&self) -> &'static str {
        if let Some(ref cause) = self.cause {
            cause.state()
        } else if self.pending {
            "loading"
        } else if self.total() == 0 {
            "empty"
        } else {
            "ready"
        }
    }

    pub fn facts(&self) -> Facts {
        let at = self
            .cursor
            .checked_sub(self.held.start)
            .and_then(|at| self.held.rows.get(at));
        Facts {
            location: self.location.clone(),
            total: self.total(),
            first: self.held.start,
            held: self.held.rows.len(),
            names: self.held.rows.iter().map(|row| row.name.clone()).collect(),
            cursor: self.cursor,
            state: self.state(),
            failed: self.cause.is_some(),
            cursor_file: at.filter(|row| !row.dir).map(|row| row.name.clone()),
            chip: self.chip,
            notice: self.said.clone(),
            filtered: self.filtered.clone(),
        }
    }
}
