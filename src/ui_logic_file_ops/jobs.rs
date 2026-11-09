// The Jobs card and its strip: rows keyed by id, newest on top, each block's words and buttons.
use super::{count, duration, parts, size, wire::JobRow, Part};
use crate::i18n::I18n;
use crate::json::escape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Waiting,
    Running,
    Failed,
    Done,
}

pub fn state(row: &JobRow) -> State {
    match row.state.as_str() {
        "waiting" => State::Waiting,
        // An encrypted archive asks through the password card, so its extract never reads as a failure (EY-326).
        "failed" if row.kind == "extract" && asks_password(&row.reason) => State::Done,
        "failed" => State::Failed,
        "done" => State::Done,
        _ => State::Running,
    }
}

fn asks_password(reason: &str) -> bool {
    !crate::ui_logic_archive_flash::archive::done_action(false, reason).is_empty()
}

pub fn unfinished(row: &JobRow) -> bool {
    matches!(state(row), State::Waiting | State::Running)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Jobs {
    // Newest first, the order the backend keeps; an update replaces its row where it stands.
    pub rows: Vec<JobRow>,
    pub folded: bool,
    pub error: String,
}

impl Jobs {
    // True when the snapshot changed what the card or the strip draws.
    pub fn snapshot(&mut self, rows: Vec<JobRow>) -> bool {
        let changed = self.rows != rows || !self.error.is_empty();
        self.rows = rows;
        self.error.clear();
        changed
    }

    // A known id keeps its place; a new one enters at the top so the first job's Cancel never moves.
    // False when the row is the one already held, so an unchanged block is never redrawn.
    pub fn update(&mut self, row: JobRow) -> bool {
        match self.rows.iter_mut().find(|r| r.id == row.id) {
            Some(slot) if *slot == row => false,
            Some(slot) => {
                *slot = row;
                true
            }
            None => {
                self.rows.insert(0, row);
                true
            }
        }
    }

    pub fn refused(&mut self, error: &str) {
        self.error = error.into();
    }

    // Done rows are notifications, not blocks: the card draws what is waiting, running or failed.
    pub fn blocks(&self) -> impl Iterator<Item = &JobRow> {
        self.rows.iter().filter(|r| state(r) != State::Done)
    }

    pub fn unfinished(&self) -> usize {
        self.rows.iter().filter(|r| unfinished(r)).count()
    }

    // The strip names the oldest unfinished job, the one the board draws.
    pub fn oldest(&self) -> Option<&JobRow> {
        self.rows.iter().rev().find(|r| unfinished(r))
    }

    pub fn visible(&self) -> bool {
        self.blocks().next().is_some()
    }

    // Folding hides the card and leaves the strip's words exactly as they were.
    pub fn toggle_fold(&mut self) {
        self.folded = !self.folded;
    }

    // The newest done row that offers Undo while nothing else runs is the one an Undo names.
    pub fn undo_target(&self) -> Option<u64> {
        if self.unfinished() > 0 {
            return None;
        }
        self.rows
            .iter()
            .find(|r| state(r) == State::Done)
            .filter(|r| r.undo)
            .map(|r| r.id)
    }
}

// The strip: the oldest unfinished head, and a muted count once more than one job is unfinished.
pub fn strip(catalog: &I18n, jobs: &Jobs) -> Option<(String, String)> {
    let head = jobs.oldest()?;
    let more = jobs.unfinished();
    let tail = if more > 1 {
        catalog.trn("jobs.strip.count", super::n(more as u64))
    } else {
        String::new()
    };
    Some((head_line(catalog, head), tail))
}

pub fn head_line(catalog: &I18n, row: &JobRow) -> String {
    // An action's failed head carries its own label, so the backend's sentence stands for it.
    if state(row) == State::Failed {
        return failed_key(&row.kind).map_or_else(|| row.head.clone(), |key| catalog.tr(key));
    }
    if row.kind == "emptyTrash" {
        return catalog.tr("jobs.empty.head");
    }
    built_head(catalog, row).unwrap_or_else(|| row.head.clone())
}

// The backend's head rebuilt from the row's fields in the catalog's language; None keeps the backend's sentence.
fn built_head(catalog: &I18n, row: &JobRow) -> Option<String> {
    let at = row.at?;
    // A failed extract that asks for a password reads as done, yet its head is still the backend's failure sentence.
    if row.state == "failed" {
        return None;
    }
    let (total, target) = (super::n(row.total), row.target.as_str());
    if row.phase == "checking" {
        return Some(catalog.trn_args("jobs.head.checking", total, &[target]));
    }
    if !row.receipt.is_empty() {
        return None;
    }
    if row.kind == "extract" && row.total == 0 {
        return Some(if row.done == 0 {
            catalog.tr("archive.extracting.head_unknown")
        } else {
            catalog.trn("archive.extracting.head_unknown_count", super::n(row.done))
        });
    }
    if state(row) == State::Waiting {
        return match row.kind.as_str() {
            "copy" => Some(catalog.trn_args("jobs.head.copy_waiting", total, &[target])),
            "move" => Some(catalog.trn_args("jobs.head.move_waiting", total, &[target])),
            "compress" => Some(catalog.trn_args("jobs.head.compress_waiting", total, &[target])),
            "extract" => Some(catalog.tr("archive.extracting.head_unknown")),
            "send" => Some(catalog.tr("jobs.head.send_waiting")),
            "action" if !row.label.is_empty() => Some(row.label.clone()),
            _ => None,
        };
    }
    let (at, of) = (count(catalog, at), count(catalog, row.total));
    match row.kind.as_str() {
        "copy" => Some(catalog.tr_args("jobs.head.copying", &[&at, &of, target])),
        "move" => Some(catalog.tr_args("jobs.head.moving", &[&at, &of, target])),
        "send" => Some(catalog.tr_args("jobs.head.sending", &[&at, &of, target])),
        "extract" => Some(catalog.tr_args("archive.extracting.head", &[&at, &of])),
        "compress" => Some(catalog.trn("jobs.head.compressing", total)),
        "action" if !row.label.is_empty() => {
            Some(catalog.tr_args("jobs.head.action", &[&row.label, &at, &of]))
        }
        _ => None,
    }
}

fn failed_key(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "copy" => "jobs.failed.copy",
        "move" => "jobs.failed.move",
        "send" => "jobs.failed.send",
        "compress" => "jobs.failed.compress",
        "extract" => "jobs.failed.extract",
        "emptyTrash" => "jobs.failed.emptyTrash",
        _ => return None,
    })
}

// What a block draws under its head, in board order.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: u64,
    pub state: State,
    pub head: String,
    pub mark_dim: bool,
    pub file: String,
    pub output: Vec<String>,
    pub bar: Option<f64>,
    pub line: Vec<Part>,
    pub buttons: Vec<Action>,
    pub cancelling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Cancel,
    Retry,
    Dismiss,
}

impl Action {
    pub fn op(self) -> &'static str {
        match self {
            Action::Cancel => "cancel",
            Action::Retry => "retry",
            Action::Dismiss => "dismiss",
        }
    }
    pub fn label_key(self) -> &'static str {
        match self {
            Action::Cancel => "jobs.button.cancel",
            Action::Retry => "jobs.button.retry",
            Action::Dismiss => "jobs.button.dismiss",
        }
    }
}

pub fn request(op: &str, id: u64) -> String {
    format!("{{\"c\":\"jobs\",\"op\":\"{}\",\"id\":{id}}}", escape(op))
}

pub fn list_request() -> &'static str {
    "{\"c\":\"jobs\",\"op\":\"list\"}"
}

// Zero totals mean unknown, so no share or estimate is invented for them.
fn share(done: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        super::transfer::unit(done as f64 / total as f64)
    }
}

fn byte_line(catalog: &I18n, row: &JobRow) -> Vec<Part> {
    let (Some(bytes), Some(total)) = (row.bytes, row.total_bytes) else {
        return Vec::new();
    };
    if bytes == 0 {
        return Vec::new();
    }
    let moved = size(catalog, bytes);
    let rate = catalog.tr_args("jobs.bytes.rate", &[&size(catalog, row.rate)]);
    match (total, row.left) {
        (0, _) => parts(&catalog.tr("jobs.bytes.copied"), &[&moved, &rate]),
        (_, Some(left)) if row.rate > 0 => parts(
            &catalog.tr("jobs.bytes.of_left"),
            &[&moved, &size(catalog, total), &rate, &duration(left)],
        ),
        _ => parts(
            &catalog.tr("jobs.bytes.of"),
            &[&moved, &size(catalog, total), &rate],
        ),
    }
}

fn count_line(catalog: &I18n, row: &JobRow) -> Vec<Part> {
    let (done, total) = (count(catalog, row.done), count(catalog, row.total));
    match row.left {
        Some(left) => parts(
            &catalog.tr("jobs.empty.deleted_left"),
            &[&done, &total, &duration(left)],
        ),
        None => parts(&catalog.tr("jobs.empty.deleted"), &[&done, &total]),
    }
}

fn file_line(catalog: &I18n, row: &JobRow) -> String {
    if row.file.is_empty() {
        String::new()
    } else if row.item_bytes > 0 {
        catalog.tr_args(
            "jobs.transfer.file",
            &[&row.file, &size(catalog, row.item_bytes)],
        )
    } else {
        row.file.clone()
    }
}

// The last two lines of an action's output replace its file line once there are any.
pub fn tail_output(output: &[String]) -> Vec<String> {
    output[output.len().saturating_sub(2)..].to_vec()
}

pub fn block(catalog: &I18n, row: &JobRow) -> Block {
    let st = state(row);
    let head = head_line(catalog, row);
    let base = Block {
        id: row.id,
        state: st,
        head,
        mark_dim: st == State::Waiting,
        file: String::new(),
        output: Vec::new(),
        bar: None,
        line: Vec::new(),
        buttons: if row.state == "cancelling" {
            Vec::new()
        } else {
            vec![Action::Cancel]
        },
        cancelling: row.state == "cancelling",
    };
    match st {
        State::Failed => Block {
            file: row.reason.clone(),
            buttons: vec![Action::Retry, Action::Dismiss],
            ..base
        },
        State::Done => Block {
            buttons: Vec::new(),
            ..base
        },
        State::Waiting => Block {
            file: row.reason.clone(),
            bar: Some(0.0),
            ..base
        },
        State::Running => {
            let output = if row.kind == "action" {
                tail_output(&row.output)
            } else {
                Vec::new()
            };
            let file = if output.is_empty() {
                file_line(catalog, row)
            } else {
                String::new()
            };
            let (bar, line) = match row.kind.as_str() {
                "action" => (share(row.done, row.total), Vec::new()),
                "emptyTrash" => (share(row.done, row.total), count_line(catalog, row)),
                _ => match (row.bytes, row.total_bytes) {
                    (Some(b), Some(t)) if t > 0 => (share(b, t), byte_line(catalog, row)),
                    _ => (share(row.done, row.total), byte_line(catalog, row)),
                },
            };
            Block {
                file,
                output,
                bar: Some(bar),
                line,
                ..base
            }
        }
    }
}

// A refusal the card shows in its own words where the backend sent a code, else the backend's sentence.
pub fn refusal(catalog: &I18n, error: &str, code: &str) -> String {
    match code {
        "retry" => catalog.tr("jobs.error.retry"),
        "dismiss" => catalog.tr("jobs.error.dismiss"),
        "undo" => catalog.tr("jobs.error.undo"),
        "unknown" => catalog.tr("jobs.error.unknown"),
        "unchanged" => catalog.tr("jobs.error.unchanged"),
        "changed" => catalog.tr("jobs.error.changed"),
        "busy" => catalog.tr("jobs.error.busy"),
        _ => error.into(),
    }
}
