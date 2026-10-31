// GIO owns the shared Trash store; this worker owns only a listing and a reviewed confirmation.
use crate::backend::opsreq::OpMsg;
use crate::backend::trashdelete::Reviewed;
use crate::backend::trashmanifest::{Cancellation, Manifest, Op, Outcome, Refusal};
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::{Duration, Instant};

const SUMMARY_BUDGET_MS: u64 = 2000;

// What a request answers when it fails: the stale flag is read from the variant, never from the sentence.
enum Failure {
    Stale(Refusal),
    Said(String),
}
impl Failure {
    fn shown(&self) -> String {
        match self {
            Self::Stale(refusal) => refusal.shown(),
            Self::Said(text) => text.clone(),
        }
    }
}
impl From<String> for Failure {
    fn from(text: String) -> Self { Self::Said(text) }
}
impl From<&str> for Failure {
    fn from(text: &str) -> Self { Self::Said(text.into()) }
}
impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self { Self::Said(refusal.shown()) }
}
// A review record that does not read back is the Trash changing under the review, so it is refused as that.
fn damaged(detail: &str) -> Refusal {
    Refusal::changed(detail).logged()
}
// What GIO said and what Flea could not do with it stay in the log; a person reads one sentence.
fn unreadable(detail: impl Into<String>) -> Refusal {
    Refusal::new(Outcome::Unreadable, detail).logged()
}
fn selection_changed(detail: impl Into<String>) -> Refusal {
    Refusal::new(Outcome::SelectionChanged, detail).logged()
}
// True when the ask was to select; false once an action had begun.
fn indistinct(selecting: bool) -> Refusal {
    Refusal::new(Outcome::Indistinct(selecting), "The Trash provider did not report a stable item identity.").logged()
}
fn unchecked(op: Op) -> Refusal {
    Refusal::new(Outcome::Unchecked(op), "The backing identity for the Trash item is unavailable.").logged()
}

#[derive(Clone, Debug, PartialEq)]
struct Item {
    uri: String,
    original: String,
}
#[derive(Clone, Debug)]
struct Detail {
    item: Item,
    size: u64,
    deleted: String,
    identity: String,
    directory: bool,
    icon: String,
    backing: Option<Reviewed>,
}
struct Confirmation {
    token: usize,
    items: Manifest,
    trees: Manifest,
    listing: Vec<Item>,
    targets: Manifest,
    count: usize,
    all: bool,
}
struct Selection {
    token: usize,
    items: Manifest,
}
struct Session {
    items: Vec<Item>,
    confirmation: Option<Confirmation>,
    selection: Option<Selection>,
    next_token: usize,
    cancellation: Cancellation,
    scratch: PathBuf,
    // Mounts the loop holds slow writes on, with the mount table they were read from, so restore and delete wait for them.
    pending: Vec<PathBuf>,
    body: String,
}
impl Default for Session {
    fn default() -> Self {
        Self { items: Vec::new(), confirmation: None, selection: None, next_token: 0,
            cancellation: Cancellation::default(), scratch: std::env::temp_dir(), pending: Vec::new(), body: String::new() }
    }
}

pub struct TrashBrowser {
    tx: Sender<(String, Cancellation, Vec<PathBuf>, String)>,
    cancellation: Cancellation,
}
impl TrashBrowser {
    pub fn new(replies: Sender<OpMsg>) -> Self {
        let (tx, rx) = channel::<(String, Cancellation, Vec<PathBuf>, String)>();
        thread::spawn(move || {
            let mut session = Session::default();
            for (request, cancellation, pending, body) in rx {
                session.cancellation = cancellation;
                session.pending = pending;
                session.body = body;
                let line = session.handle(&request);
                if replies.send(OpMsg::Meta { line }).is_err() { break; }
            }
        });
        Self { tx, cancellation: Cancellation::default() }
    }
    pub fn request(&self, line: String, pending: Vec<PathBuf>, body: String) {
        let _ = self.tx.send((line, self.cancellation.next(), pending, body));
    }
}
impl Drop for TrashBrowser {
    fn drop(&mut self) { self.cancellation.next(); }
}

fn gio(args: &[&str]) -> Result<String, Refusal> {
    let prefix = super::trash::gio_prefix();
    answer(Command::new(&prefix[0])
        .env("LC_ALL", "C")
        .args(&prefix[1..])
        .args(args)
        .output())
}
fn answer(output: std::io::Result<std::process::Output>) -> Result<String, Refusal> {
    let output = output.map_err(|e| unreadable(format!("Could not start the Trash helper: {}", e)))?;
    if !output.status.success() {
        if matches!(output.status.code(), Some(124 | 137)) {
            return Err(Refusal::new(Outcome::TookTooLong, "The Trash helper exceeded its deadline.").logged());
        }
        return Err(unreadable(String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or("The gio Trash helper failed.")));
    }
    String::from_utf8(output.stdout).map_err(|_| unreadable("The Trash helper returned invalid text."))
}

// Sample input: "trash:///a.txt\t/home/gm/a.txt\n"; malformed framing must not silently lose an item.
fn parse_list(text: &str) -> Result<Vec<Item>, Refusal> {
    let mut items: Vec<Item> = Vec::new();
    for line in text.lines() {
        // An original path holding a newline wraps onto the next line; a line that opens no new trash:/// uri continues it.
        if !line.starts_with("trash:///") {
            if let Some(last) = items.last_mut() {
                last.original.push('\n');
                last.original.push_str(line);
                continue;
            }
        }
        let (uri, original) = line
            .split_once('\t')
            .ok_or_else(|| unreadable("The Trash helper returned an unreadable listing."))?;
        if !uri.starts_with("trash:///")
            || uri.len() <= "trash:///".len()
            || !original.starts_with('/')
        {
            return Err(unreadable("The Trash helper returned an invalid item identity."));
        }
        items.push(Item {
            uri: uri.into(),
            original: original.into(),
        });
    }
    items.sort_by(|a, b| a.uri.cmp(&b.uri));
    if items.windows(2).any(|pair| pair[0].uri == pair[1].uri) {
        return Err(unreadable("The Trash helper returned duplicate item identities."));
    }
    Ok(items)
}
fn list() -> Result<Vec<Item>, Refusal> {
    list_with(gio)
}

// The listing over any reader of gio's text, so a test can feed the two outputs through the real join.
fn list_with(read: impl Fn(&[&str]) -> Result<String, Refusal>) -> Result<Vec<Item>, Refusal> {
    let mut items = parse_list(&read(&["trash", "--list"])?)?;
    // One gio call dates every volume's items; a failed call leaves the order by uri.
    let dates = parse_dates(&read(&["list", "-u", "-a", "trash::deletion-date", "trash:///"]).unwrap_or_default());
    newest_first(&mut items, |item| dates.get(&item.uri).cloned().unwrap_or_default());
    Ok(items)
}
// Two reads of one Trash hold the same items whatever order each came in: a dates call that failed in one read orders it by uri.
fn same_items(a: &[Item], b: &[Item]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort_by(|x, y| x.uri.cmp(&y.uri));
    b.sort_by(|x, y| x.uri.cmp(&y.uri));
    a == b
}
// Membership by uri, not by position: the listing is in date order, so a uri search over it would miss items.
fn listed_uris(items: &[Item]) -> HashSet<&str> {
    items.iter().map(|item| item.uri.as_str()).collect()
}

// Sample input, gio list -u -a trash::deletion-date trash:///: "trash:///a.txt\t0\t(regular)\ttrash::deletion-date=2026-09-08T10:00:00\n"; local clock time with no offset, so the repeated fall-back hour can order two items wrongly, accepted because gio gives no offset.
fn parse_dates(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let uri = fields.next()?;
            let date = fields.find_map(|field| field.strip_prefix("trash::deletion-date="))?;
            Some((uri.to_string(), date.to_string()))
        })
        .collect()
}
// Newest deletion first, ties by uri, so the order is total and two listings of one Trash agree.
fn newest_first(items: &mut Vec<Item>, date_of: impl Fn(&Item) -> String) {
    let mut dated: Vec<(String, Item)> = items.drain(..).map(|item| (date_of(&item), item)).collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.uri.cmp(&b.1.uri)));
    items.extend(dated.into_iter().map(|(_, item)| item));
}

// Sample input, gio info: "  standard::size: 123\n  trash::deletion-date: 2026-09-08T10:00:00".
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        line.trim_start()
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix(": "))
    })
}
fn detail(item: &Item) -> Result<Detail, Refusal> {
    let text = gio(&["info", "--nofollow-symlinks", "--attributes=standard::size,standard::type,trash::deletion-date,id::file,id::filesystem,standard::icon,standard::target-uri", &item.uri])?;
    let size = attribute(&text, "standard::size")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| unreadable("The Trash helper did not report the item's size."))?;
    let identity = match (
        attribute(&text, "id::filesystem"),
        attribute(&text, "id::file"),
    ) {
        (Some(fs), Some(file)) if !fs.is_empty() && !file.is_empty() => format!("{}:{}", fs, file),
        _ => String::new(),
    };
    let backing = attribute(&text, "standard::target-uri")
        .and_then(|uri| uri.strip_prefix("file://"))
        .and_then(|path| {
            Reviewed::inspect(
                PathBuf::from(crate::paths::percent_decode(path)),
                attribute(&text, "id::file").unwrap_or(""),
            )
            .ok()
        });
    Ok(Detail {
        backing,
        item: item.clone(),
        size,
        deleted: attribute(&text, "trash::deletion-date")
            .unwrap_or("")
            .into(),
        identity,
        directory: attribute(&text, "standard::type") == Some("2"),
        icon: attribute(&text, "standard::icon")
            .unwrap_or("")
            .split(", ")
            .next()
            .unwrap_or("")
            .into(),
    })
}
fn row(item: &Detail) -> String {
    format!(
        r#"{{"uri":"{}","original":"{}","size":{},"deleted":"{}","directory":{},"icon":"{}","identity":"{}"}}"#,
        escape(&item.item.uri),
        escape(&item.item.original),
        item.size,
        escape(&item.deleted),
        item.directory,
        escape(&item.icon),
        escape(&selection_identity(item))
    )
}

fn selection_identity(item: &Detail) -> String {
    format!(r#"["{}","{}",{},"{}","{}","{}"]"#,
        escape(&item.item.uri), escape(&item.item.original), item.size,
        escape(&item.deleted), escape(&item.identity),
        escape(&item.backing.as_ref().map(Reviewed::selection_identity).unwrap_or_default()))
}

// A window or a validity check asks this without refusing anything, so only the refusal below logs.
fn selection_current(item: &Detail, expected: &str) -> bool {
    !(expected.is_empty() || item.identity.is_empty() || selection_identity(item) != expected)
}
fn require_selected_identity(item: &Detail, expected: &str) -> Result<(), Refusal> {
    if !selection_current(item, expected) {
        return Err(selection_changed(format!("Trash selection changed for {}.", item.item.uri)));
    }
    Ok(())
}

fn record_text(bytes: Vec<u8>) -> Result<String, Refusal> {
    String::from_utf8(bytes).map_err(|_| damaged("Invalid text in Trash review."))
}
fn record_item(text: &str) -> Result<Item, Refusal> {
    Ok(Item { uri: field_str(text, "uri").ok_or_else(|| damaged("Missing reviewed Trash URI."))?,
        original: field_str(text, "original").ok_or_else(|| damaged("Missing reviewed original path."))? })
}
fn save_detail(item: &Detail) -> Result<String, Refusal> {
    let mut text = row(item);
    text.pop();
    text.push_str(&format!(r#","providerIdentity":"{}","backing":"{}"}}"#,
        escape(&item.identity), escape(&item.backing.as_ref().ok_or_else(|| damaged("Missing Trash backing review."))?.saved()?)));
    Ok(text)
}
fn load_detail(text: &str, trees: &Manifest) -> Result<Detail, Refusal> {
    Ok(Detail {
        item: record_item(text)?,
        size: field_usize(text, "size").ok_or_else(|| damaged("Missing reviewed Trash size."))? as u64,
        deleted: field_str(text, "deleted").unwrap_or_default(),
        identity: field_str(text, "providerIdentity").ok_or_else(|| damaged("Missing reviewed provider identity."))?,
        directory: field_bool(text, "directory"),
        icon: field_str(text, "icon").unwrap_or_default(),
        backing: Some(Reviewed::from_saved(&field_str(text, "backing").ok_or_else(|| damaged("Missing Trash backing review."))?, trees)?),
    })
}
fn failure(item: &Item, expected: &str, error: &str) -> String {
    format!(r#"{{"uri":"{}","name":"{}","identity":"{}","error":"{}"}}"#,
        escape(&item.uri), escape(item.original.rsplit('/').next().unwrap_or(&item.original)), escape(expected), escape(error))
}
fn delete_one(item: &Detail, recovery: &Result<PathBuf, String>) -> Result<(), String> {
    let reviewed = item.backing.as_ref().ok_or_else(|| unchecked(Op::Delete).shown())?;
    reviewed.delete(recovery.as_ref().map_err(Clone::clone)?).map_err(String::from)
}
// A record that will not read back fails its own item, so items already deleted stay counted; a record with no readable uri gets an empty one.
fn unread(item: Option<Item>) -> String {
    let item = item.unwrap_or(Item { uri: String::new(), original: String::new() });
    failure(&item, "", "One Trash record could not be read, so that item was left in place.")
}
fn unread_many(count: usize) -> String {
    if count == 1 { return unread(None); }
    failure(&Item { uri: String::new(), original: String::new() }, "", &format!("{count} Trash records could not be read, so those items were left in place."))
}
// Prepare appends one item per target in target order and fails whole on any error, so item `reached` is target `reached`; past a damaged frame targets are read backward and the stretch between is counted.
fn left_unread(confirmation: &Confirmation, reached: usize) -> (Vec<String>, usize) {
    let left = confirmation.count.saturating_sub(reached).max(1);
    let (mut rows, mut unnamed, mut offset) = (Vec::new(), 0, 0);
    let name = |bytes: Vec<u8>, rows: &mut Vec<String>, unnamed: &mut usize| match record_text(bytes).ok().and_then(|text| record_item(&text).ok()) {
        Some(item) => rows.push(unread(Some(item))),
        None => *unnamed += 1,
    };
    let records = confirmation.targets.records();
    let mut index = 0;
    while let Ok(Some(bytes)) = records.next(&mut offset) {
        if index >= reached { name(bytes, &mut rows, &mut unnamed); }
        index += 1;
    }
    let mut back = records.end();
    if offset != back {
        let mut from_end = confirmation.count;
        while back > offset {
            let Ok(Some(bytes)) = records.previous(&mut back) else { break };
            from_end = from_end.saturating_sub(1);
            if from_end >= reached { name(bytes, &mut rows, &mut unnamed); }
        }
    }
    unnamed += left.saturating_sub(rows.len() + unnamed);
    if unnamed > 0 { rows.push(unread_many(unnamed)); }
    (rows, left)
}
fn delete_confirmed(confirmation: &Confirmation, recovery: &Result<PathBuf, String>) -> String {
    let mut done = 0;
    let mut failures = Vec::new();
    let mut offset = 0;
    let mut reached = 0;
    let mut folded = 0;
    loop {
        let bytes = match confirmation.items.records().next(&mut offset) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            // The offset does not move past a frame that cannot be read, so the records after it cannot be found there.
            Err(error) => {
                damaged(&error);
                let (rows, left) = left_unread(confirmation, reached);
                folded += left.saturating_sub(rows.len());
                failures.extend(rows);
                break;
            }
        };
        reached += 1;
        let text = match record_text(bytes) {
            Ok(text) => text,
            Err(_) => { failures.push(unread(None)); continue; }
        };
        match load_detail(&text, &confirmation.trees) {
            Ok(item) => match delete_one(&item, recovery) {
                Ok(()) => done += 1,
                Err(error) => failures.push(failure(&item.item, &surviving_identity(&item), &error)),
            },
            Err(_) => failures.push(unread(record_item(&text).ok())),
        }
    }
    // `failed` counts items, and a row for records that could not be read stands for several of them.
    format!(r#""done":{},"failed":{},"failures":[{}]"#, done, failures.len() + folded, failures.join(","))
}
fn surviving_identity(item: &Detail) -> String {
    if let Ok(current) = detail(&item.item) {
        if let (Some(old), Some(new)) = (&item.backing, &current.backing) {
            if old.same_item(new) { return selection_identity(&current); }
        }
    }
    selection_identity(item)
}

impl Session {
    // Sample input: {"c":"trashbrowse","op":"prepare","id":1,"uris":["trash:///a.txt"],"all":false}.
    fn handle(&mut self, line: &str) -> String {
        let id = field_usize(line, "id").unwrap_or(0);
        let op = field_str(line, "op").unwrap_or_default();
        match self.execute(&op, line) {
            Ok(fields) => format!(
                r#"{{"t":"trashbrowse","id":{},"op":"{}","ok":true,{}}}"#,
                id,
                escape(&op),
                fields
            ),
            Err(failure) => format!(
                r#"{{"t":"trashbrowse","id":{},"op":"{}","ok":false,"stale":{},"error":"{}"}}"#,
                id,
                escape(&op),
                op == "delete" && matches!(failure, Failure::Stale(_)),
                escape(&failure.shown())
            ),
        }
    }
    fn execute(&mut self, op: &str, line: &str) -> Result<String, Failure> {
        self.cancellation.check()?;
        match op {
            "list" => {
                let recovery = if field_bool(line, "recover") {
                    crate::backend::trashdelete::recover(&crate::backend::trashdelete::recovery_root()?)?
                } else { crate::backend::trashdelete::RecoveryReport::default() };
                self.items = list()?;
                let window = self.window(
                    field_usize(line, "start").unwrap_or(0),
                    field_usize(line, "count").unwrap_or(100),
                )?;
                let selected: HashSet<_> = field_str_array(line, "uris").into_iter().collect();
                let present = self.items.iter().filter(|item| selected.contains(&item.uri))
                    .map(|item| format!("\"{}\"", escape(&item.uri))).collect::<Vec<_>>();
                Ok(format!("{},\"present\":[{}],\"recoveredCount\":{},\"recoveryErrors\":[{}]", window, present.join(","),
                    recovery.recovered, recovery.failures.iter().map(|error| format!("\"{}\"", escape(error))).collect::<Vec<_>>().join(",")))
            }
            "summary" => Ok(self.summary()?),
            "select" => Ok(self.select_all()?),
            "window" => Ok(self.window(
                field_usize(line, "start").unwrap_or(0),
                field_usize(line, "count").unwrap_or(100),
            )?),
            "prepare" => Ok(self.prepare(line)?),
            "check" => {
                let valid = self.valid(field_usize(line, "token").unwrap_or(0))?;
                Ok(format!(r#""valid":{}"#, valid))
            }
            "cancel" => {
                self.confirmation = None;
                if field_bool(line, "clearSelection") { self.selection = None; }
                Ok(r#""cancelled":true"#.into())
            }
            "restore" => Ok(self.restore(line)?),
            "delete" => {
                // A target whose original sits on a pending mount waits, checked before any listing so the refusal needs no trash round trip.
                if let Some(confirmation) = self.confirmation.as_ref().filter(|c| c.token == field_usize(line, "token").unwrap_or(0)) {
                    if self.targets_blocked(&confirmation.targets)? {
                        return Err("an operation is already running".into());
                    }
                }
                if !self.valid(field_usize(line, "token").unwrap_or(0))? {
                    return Err(Failure::Stale(Refusal::new(Outcome::ConfirmAgain, "Trash changed; confirm again.").logged()));
                }
                let confirmation = self
                    .confirmation
                    .take()
                    .ok_or("Trash confirmation expired.")?;
                Ok(delete_confirmed(&confirmation, &crate::backend::trashdelete::recovery_root()))
            }
            _ => Err("Unknown Trash request.".into()),
        }
    }
    fn window(&mut self, start: usize, count: usize) -> Result<String, String> {
        let start = start.min(self.items.len().saturating_sub(1));
        let mut rows = Vec::new();
        for item in self.items.iter().skip(start).take(count.min(350)) {
            self.cancellation.check()?;
            rows.push(detail(item)?);
        }
        let visible: HashMap<_, _> = rows.iter().map(|item| (&item.item.uri, item)).collect();
        let mut members = HashSet::new();
        let mut selected_count = 0;
        let mut stale = false;
        let token = self.selection.as_ref().map(|selection| selection.token).unwrap_or(0);
        if let Some(selection) = &self.selection {
            let listed = listed_uris(&self.items);
            let mut offset = 0;
            while let Some(bytes) = selection.items.records().next(&mut offset)? {
                self.cancellation.check()?;
                let text = record_text(bytes)?;
                let item = record_item(&text)?;
                if !listed.contains(item.uri.as_str()) { continue; }
                selected_count += 1;
                if let Some(current) = visible.get(&item.uri) {
                    let expected = field_str(&text, "identity").unwrap_or_default();
                    if !selection_current(current, &expected) { stale = true; }
                    else { members.insert(item.uri); }
                }
            }
        }
        if stale { self.selection = None; selected_count = 0; members.clear(); }
        let rows = rows.iter().map(|item| {
            let mut text = row(item);
            text.pop();
            text.push_str(&format!(r#","bulkSelected":{}}}"#, members.contains(&item.item.uri)));
            text
        }).collect::<Vec<_>>();
        Ok(format!(r#""total":{},"start":{},"rows":[{}],"selectionToken":{},"selectionCount":{},"selectionStale":{}"#,
            self.items.len(), start, rows.join(","), if stale { 0 } else { token }, selected_count, stale))
    }
    fn select_all(&mut self) -> Result<String, String> {
        if self.confirmation.is_some() { return Err("Finish or cancel the Trash confirmation before selecting items.".into()); }
        self.selection = None;
        self.items = list()?;
        let mut records = Manifest::new(&self.scratch)?;
        for item in &self.items {
            self.cancellation.check()?;
            let current = detail(item)?;
            if current.identity.is_empty() || current.backing.is_none() {
                return Err(indistinct(true).into());
            }
            records.append(row(&current).as_bytes())?;
        }
        if !same_items(&list()?, &self.items) { return Err("Trash changed while selecting its contents; select again.".into()); }
        let mut offset = 0;
        while let Some(bytes) = records.records().next(&mut offset)? {
            self.cancellation.check()?;
            let text = record_text(bytes)?;
            require_selected_identity(&detail(&record_item(&text)?)?, &field_str(&text, "identity").unwrap_or_default())?;
        }
        self.next_token += 1;
        self.selection = Some(Selection { token: self.next_token, items: records });
        Ok(format!(r#""selectionToken":{},"selectionCount":{}"#, self.next_token, self.items.len()))
    }
    fn summary(&self) -> Result<String, String> {
        let deadline = Instant::now() + Duration::from_millis(SUMMARY_BUDGET_MS);
        let mut bytes = 0u64;
        let mut partial = false;
        for item in &self.items {
            self.cancellation.check()?;
            if Instant::now() >= deadline {
                partial = true;
                break;
            }
            let detail = detail(item)?;
            match detail.backing {
                Some(backing) => {
                    let size = backing.bytes(deadline);
                    bytes += size.bytes;
                    partial |= size.partial;
                }
                None => {
                    bytes += detail.size;
                    partial |= detail.directory;
                }
            }
        }
        Ok(format!(r#""bytes":{},"partial":{}"#, bytes, partial))
    }
    fn targets(&self, line: &str) -> Result<Manifest, String> {
        let mut targets = Manifest::new(&self.scratch)?;
        let uris = field_str_array(line, "uris");
        let identities = field_str_array(line, "identities");
        let all = field_bool(line, "all");
        if !all && uris.len() != identities.len() {
            return Err(selection_changed("The reviewed identity for the Trash selection is missing.").into());
        }
        let mut explicit: HashMap<_, _> = uris.iter().zip(&identities).collect();
        let exclusions: HashSet<_> = field_str_array(line, "exclude").into_iter().collect();
        let token = field_usize(line, "selectionToken").unwrap_or(0);
        if all {
            for item in &self.items {
                self.cancellation.check()?;
                targets.append(format!(r#"{{"uri":"{}","original":"{}","identity":""}}"#,
                    escape(&item.uri), escape(&item.original)).as_bytes())?;
            }
        } else if token > 0 {
            let selection = self.selection.as_ref().filter(|selection| selection.token == token)
                .ok_or_else(|| selection_changed("The saved Trash selection is no longer current."))?;
            let mut offset = 0;
            while let Some(bytes) = selection.items.records().next(&mut offset)? {
                self.cancellation.check()?;
                let text = record_text(bytes)?;
                let item = record_item(&text)?;
                if exclusions.contains(&item.uri) { continue; }
                if explicit.contains_key(&item.uri) { continue; }
                targets.append(text.as_bytes())?;
            }
        }
        if !all {
            for uri in &uris {
                let Some(expected) = explicit.remove(uri) else { continue; };
                let item = self.items.iter().find(|item| &item.uri == uri)
                    .ok_or_else(|| selection_changed("The saved Trash selection is no longer current."))?;
                if expected.is_empty() { return Err(selection_changed("The reviewed identity for the Trash selection is missing.").into()); }
                targets.append(format!(r#"{{"uri":"{}","original":"{}","identity":"{}"}}"#,
                    escape(uri), escape(&item.original), escape(expected)).as_bytes())?;
            }
        }
        if targets.len() == 0 { return Err(if all { "Trash is empty." } else { "Select at least one Trash item." }.into()); }
        Ok(targets)
    }
    // Originals behind trash uris, checked against the mounts the loop holds slow writes on.
    fn targets_blocked(&self, targets: &Manifest) -> Result<bool, String> {
        let mut offset = 0;
        while let Some(bytes) = targets.records().next(&mut offset)? {
            let item = record_item(&record_text(bytes)?)?;
            let mount = crate::backend::iomount::mount_key(&PathBuf::from(&item.original), &self.body);
            if self.pending.iter().any(|held| held == &mount) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn refreshed_targets(&self, previous: &Confirmation) -> Result<Manifest, String> {
        if previous.all { return self.targets(r#"{"all":true}"#); }
        let mut targets = Manifest::new(&self.scratch)?;
        let mut cursor = 0;
        while let Some(bytes) = previous.targets.records().next(&mut cursor)? {
            self.cancellation.check()?;
            let old = record_item(&record_text(bytes)?)?;
            if let Some(current) = self.items.iter().find(|item| item.uri == old.uri) {
                targets.append(format!(r#"{{"uri":"{}","original":"{}","identity":""}}"#,
                    escape(&current.uri), escape(&current.original)).as_bytes())?;
            }
        }
        if targets.len() == 0 { return Err("The selected Trash items are no longer present.".into()); }
        Ok(targets)
    }
    fn prepare(&mut self, line: &str) -> Result<String, String> {
        let refresh_token = field_usize(line, "refreshToken").unwrap_or(0);
        self.items = list()?;
        let previous = self.confirmation.take();
        let all = field_bool(line, "all");
        let targets = if refresh_token > 0 {
            let previous = previous.as_ref().filter(|previous| previous.token == refresh_token && previous.all == all)
                .ok_or("Trash confirmation expired; start the action again.")?;
            self.refreshed_targets(previous)?
        } else { self.targets(line)? };
        let mut items = Manifest::new(&self.scratch)?;
        let mut trees = Manifest::new(&self.scratch)?;
        let mut cursor = 0;
        let mut count = 0;
        let mut bytes = 0u64;
        while let Some(record) = targets.records().next(&mut cursor)? {
            self.cancellation.check()?;
            let text = record_text(record)?;
            let mut current = detail(&record_item(&text)?)?;
            if !all && refresh_token == 0 {
                require_selected_identity(&current, &field_str(&text, "identity").unwrap_or_default())?;
            }
            if current.identity.is_empty() { return Err(indistinct(false).into()); }
            let size = current.backing.as_mut()
                .ok_or_else(|| unchecked(Op::Delete))?
                .snapshot(&mut trees, &self.scratch, &self.cancellation)?;
            bytes = bytes.checked_add(size).ok_or("Trash byte total exceeds the supported range.")?;
            items.append(save_detail(&current)?.as_bytes())?;
            count += 1;
        }
        self.next_token += 1;
        self.confirmation = Some(Confirmation { token: self.next_token, items, trees, listing: self.items.clone(), targets, count, all });
        if !self.valid(self.next_token)? {
            self.confirmation = None;
            return Err("Trash changed while reviewing its contents; try the action again.".into());
        }
        Ok(format!(r#""token":{},"all":{},"count":{},"bytes":{},"partial":false"#,
            self.next_token, field_bool(line, "emptyTrash"), count, bytes))
    }
    fn valid(&self, token: usize) -> Result<bool, String> {
        let confirmation = match self.confirmation.as_ref() {
            Some(c) if c.token == token => c,
            _ => return Ok(false),
        };
        if !same_items(&list()?, &confirmation.listing) { return Ok(false); }
        let mut cursor = 0;
        while let Some(bytes) = confirmation.items.records().next(&mut cursor)? {
            self.cancellation.check()?;
            let old = load_detail(&record_text(bytes)?, &confirmation.trees)?;
            let Some(backing) = old.backing.as_ref() else { return Ok(false); };
            if !backing.contents_unchanged(&self.cancellation)? { return Ok(false); }
            let current = detail(&old.item)?;
            if !selection_current(&current, &selection_identity(&old)) { return Ok(false); }
        }
        Ok(true)
    }
    fn restore(&mut self, line: &str) -> Result<String, String> {
        if field_bool(line, "all") { self.items = list()?; }
        let targets = self.targets(line)?;
        // A target whose original sits on a pending mount waits, so the late write lands first.
        if self.targets_blocked(&targets)? {
            return Err("an operation is already running".into());
        }
        let mut done = 0;
        let mut failures = Vec::new();
        let mut cursor = 0;
        while let Some(bytes) = targets.records().next(&mut cursor)? {
            let text = record_text(bytes)?;
            let item = record_item(&text)?;
            let mut retained = field_str(&text, "identity").unwrap_or_default();
            let restored = detail(&item).map_err(String::from).and_then(|current| {
                if !field_bool(line, "all") {
                    require_selected_identity(&current, &field_str(&text, "identity").unwrap_or_default())?;
                }
                let result = current.backing.as_ref()
                    .ok_or_else(|| unchecked(Op::Restore))?
                    .restore(&PathBuf::from(&item.original), &crate::backend::trashdelete::recovery_root()?)
                    .map_err(String::from);
                if result.is_err() { retained = surviving_identity(&current); }
                result
            });
            match restored {
                Ok(_) => done += 1,
                Err(error) => failures.push(failure(&item, &retained, &error)),
            }
        }
        self.confirmation = None;
        self.selection = None;
        Ok(format!(r#""done":{},"failed":{},"failures":[{}]"#, done, failures.len(), failures.join(",")))
    }

}

#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_list_joins_an_original_path_wrapped_by_a_newline() {
        let items = parse_list("trash:///a\t/home/x/P5)\nlines.append(\ntrash:///b\t/home/x/b\n").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].original, "/home/x/P5)\nlines.append(");
    }
    #[test]
    fn malformed_or_duplicate_listing_fails_closed() {
        assert!(parse_list("").unwrap().is_empty());
        for text in [
            "bad",
            "file:///x\t/x",
            "trash:///\t/x",
            "trash:///a\trelative",
            "trash:///a\t/a\ntrash:///a\t/a",
        ] {
            assert!(parse_list(text).is_err());
        }
        assert_eq!(
            parse_list("trash:///a\t/tmp/a b\n").unwrap()[0].original,
            "/tmp/a b"
        );
    }
    #[test]
    fn listing_is_newest_deleted_first_and_ties_break_by_uri() {
        let item = |name: &str| Item { uri: format!("trash:///{}", name), original: format!("/home/u/{}", name) };
        let mut items = vec![item("b.txt"), item("a.txt"), item("c.txt")];
        let date = |item: &Item| match item.uri.as_str() {
            "trash:///a.txt" => "2026-10-05T09:00:00".to_string(),
            _ => "2026-10-02T10:00:00".to_string(),
        };
        newest_first(&mut items, date);
        let order: Vec<_> = items.iter().map(|item| item.uri.as_str()).collect();
        assert_eq!(order, ["trash:///a.txt", "trash:///b.txt", "trash:///c.txt"]);
        // A listing in the other uri order lands on the same sequence, so two reads of one Trash agree.
        let mut reversed = vec![item("c.txt"), item("b.txt"), item("a.txt")];
        newest_first(&mut reversed, date);
        assert_eq!(reversed, items);
    }
    #[test]
    fn deletion_dates_come_from_one_gio_list() {
        let text = "trash:///a%20b.txt\t0\t(regular)\ttrash::deletion-date=2026-09-08T10:00:00\ntrash:///c\t4096\t(directory)\ntrash:///d\t1\t(regular)\ttrash::deletion-date=2026-09-09T11:30:00\n";
        let dates = parse_dates(text);
        assert_eq!(dates.get("trash:///a%20b.txt").map(String::as_str), Some("2026-09-08T10:00:00"));
        assert_eq!(dates.get("trash:///d").map(String::as_str), Some("2026-09-09T11:30:00"));
        assert_eq!(dates.get("trash:///c"), None);
        assert!(parse_dates("").is_empty());
    }
    #[test]
    fn an_item_without_a_date_sorts_last() {
        let item = |name: &str| Item { uri: format!("trash:///{}", name), original: format!("/home/u/{}", name) };
        let mut items = vec![item("a"), item("b"), item("c")];
        newest_first(&mut items, |item| if item.uri.ends_with('b') { "2026-10-01T00:00:00".into() } else { String::new() });
        let order: Vec<_> = items.iter().map(|item| item.uri.as_str()).collect();
        assert_eq!(order, ["trash:///b", "trash:///a", "trash:///c"]);
    }
    // gio's two outputs for one Trash, one escaped name in both: the listing in uri order, the dates by uri.
    const TRASH_LIST: &str = "trash:///a%20b.txt\t/home/u/a b.txt\ntrash:///c.txt\t/home/u/c.txt\ntrash:///z.txt\t/home/u/z.txt\n";
    const TRASH_DATES: &str = "trash:///a%20b.txt\t0\t(regular)\ttrash::deletion-date=2026-10-05T09:00:00\ntrash:///c.txt\t0\t(regular)\ttrash::deletion-date=2026-10-01T09:00:00\ntrash:///z.txt\t0\t(regular)\ttrash::deletion-date=2026-10-03T09:00:00\n";
    fn read_gio(dates: Result<&'static str, &'static str>) -> impl Fn(&[&str]) -> Result<String, Refusal> {
        move |args: &[&str]| match args[0] {
            "trash" => Ok(TRASH_LIST.to_string()),
            _ => dates.map(String::from).map_err(unreadable),
        }
    }
    fn uris(items: &[Item]) -> Vec<&str> {
        items.iter().map(|item| item.uri.as_str()).collect()
    }
    #[test]
    fn the_real_listing_joins_dates_to_uris_newest_first() {
        let items = list_with(read_gio(Ok(TRASH_DATES))).unwrap();
        assert_eq!(uris(&items), ["trash:///a%20b.txt", "trash:///z.txt", "trash:///c.txt"]);
        assert_eq!(items[0].original, "/home/u/a b.txt");
    }
    #[test]
    fn a_failed_dates_call_keeps_the_uri_order() {
        let items = list_with(read_gio(Err("gio failed"))).unwrap();
        assert_eq!(uris(&items), ["trash:///a%20b.txt", "trash:///c.txt", "trash:///z.txt"]);
    }
    #[test]
    fn a_failed_trash_list_is_an_error() {
        assert!(list_with(|_: &[&str]| Err(unreadable("gio failed"))).is_err());
    }
    #[test]
    fn two_reads_of_one_trash_are_unchanged_whatever_their_order() {
        let dated = list_with(read_gio(Ok(TRASH_DATES))).unwrap();
        let by_uri = list_with(read_gio(Err("timed out"))).unwrap();
        assert_ne!(dated, by_uri);
        assert!(same_items(&dated, &by_uri));
        assert!(same_items(&by_uri, &dated));
        // A different item or a different original is a change.
        let mut other = by_uri.clone();
        other[1].original = "/home/u/elsewhere".into();
        assert!(!same_items(&dated, &other));
        assert!(!same_items(&dated, &by_uri[1..]));
    }
    #[test]
    fn membership_finds_every_item_of_a_date_ordered_listing() {
        let items = list_with(read_gio(Ok(TRASH_DATES))).unwrap();
        assert_ne!(uris(&items), { let mut sorted = uris(&items); sorted.sort(); sorted });
        let listed = listed_uris(&items);
        for item in &items { assert!(listed.contains(item.uri.as_str()), "{} missed", item.uri); }
        assert!(!listed.contains("trash:///absent"));
    }
    #[test]
    fn selected_targets_require_known_paths_and_original_identities() {
        let d = crate::backend::testdir::TestDir::new("trash-selected-targets");
        let session = Session {
            items: parse_list("trash:///a\t/tmp/a\n").unwrap(),
            scratch: d.path().to_path_buf(),
            ..Session::default()
        };
        for request in [r#"{"uris":["/tmp/a"],"identities":["saved"]}"#,
            r#"{"uris":["trash:///a"],"identities":[]}"#,
            r#"{"uris":["trash:///a"],"identities":[""]}"#,
            r#"{"selectionToken":1}"#] {
            assert!(session.targets(request).is_err());
        }
        let targets = session.targets(r#"{"uris":["trash:///a","trash:///a"],"identities":["saved","saved"]}"#).unwrap();
        let mut offset = 0;
        assert!(targets.records().next(&mut offset).unwrap().is_some());
        assert!(targets.records().next(&mut offset).unwrap().is_none());
    }
    #[test]
    fn bulk_targets_keep_the_saved_set_and_identity_when_listing_changes() {
        let d = crate::backend::testdir::TestDir::new("trash-bulk-targets");
        let mut selected = Manifest::new(d.path()).unwrap();
        selected.append(br#"{"uri":"trash:///a","original":"/original/a","identity":"original-inode"}"#).unwrap();
        let session = Session {
            items: parse_list("trash:///a\t/replacement/a\ntrash:///new\t/new\n").unwrap(),
            selection: Some(Selection { token: 7, items: selected }),
            scratch: d.path().to_path_buf(),
            ..Session::default()
        };
        let targets = session.targets(r#"{"selectionToken":7}"#).unwrap();
        let mut offset = 0;
        let record = record_text(targets.records().next(&mut offset).unwrap().unwrap()).unwrap();
        assert_eq!(field_str(&record, "identity").unwrap(), "original-inode");
        assert_eq!(record_item(&record).unwrap().original, "/original/a");
        assert!(targets.records().next(&mut offset).unwrap().is_none());
        assert!(session.targets(r#"{"selectionToken":7,"exclude":["trash:///a"]}"#).is_err());
    }
    #[test]
    fn refreshed_confirmation_keeps_its_uri_set_without_rewriting_selection() {
        let d = crate::backend::testdir::TestDir::new("trash-confirm-refresh");
        let mut targets = Manifest::new(d.path()).unwrap();
        targets.append(br#"{"uri":"trash:///a","original":"/old/a","identity":"old-inode"}"#).unwrap();
        targets.append(br#"{"uri":"trash:///gone","original":"/old/gone","identity":"gone-inode"}"#).unwrap();
        let confirmation = Confirmation { token: 3, items: Manifest::new(d.path()).unwrap(),
            trees: Manifest::new(d.path()).unwrap(), listing: Vec::new(), targets, count: 0, all: false };
        let session = Session { items: parse_list("trash:///a\t/new/a\ntrash:///new\t/new\n").unwrap(),
            scratch: d.path().to_path_buf(), ..Session::default() };
        let refreshed = session.refreshed_targets(&confirmation).unwrap();
        let mut offset = 0;
        let text = record_text(refreshed.records().next(&mut offset).unwrap().unwrap()).unwrap();
        assert_eq!(record_item(&text).unwrap().original, "/new/a");
        assert_eq!(field_str(&text, "identity").unwrap(), "");
        assert!(refreshed.records().next(&mut offset).unwrap().is_none());
        let mut offset = 0;
        let held = record_text(confirmation.targets.records().next(&mut offset).unwrap().unwrap()).unwrap();
        assert_eq!(field_str(&held, "identity").unwrap(), "old-inode");
    }
    #[test]
    fn named_failure_keeps_its_retry_identity() {
        let item = Item { uri: "trash:///a".into(), original: "/original/a".into() };
        let message = failure(&item, "held-identity", "Permission denied");
        assert_eq!(field_str(&message, "identity").unwrap(), "held-identity");
        assert_eq!(field_str(&message, "name").unwrap(), "a");
        assert_eq!(field_str(&message, "error").unwrap(), "Permission denied");
    }
    #[test]
    fn expired_confirmation_never_reaches_provider() {
        assert!(!Session::default().valid(1).unwrap());
    }
    // The flag comes from the typed failure, so a sentence that reads the same on another op never sets it.
    #[test]
    fn a_stale_delete_sets_the_flag_and_says_confirm_again_with_its_detail_logged() {
        let seen = crate::backend::trashmanifest::logged().len();
        let stale = Session::default().handle(r#"{"c":"trashbrowse","op":"delete","id":4,"token":9}"#);
        assert!(stale.contains(r#""ok":false,"stale":true,"error":"The Trash changed, so nothing was done; confirm again.""#), "{}", stale);
        assert!(crate::backend::trashmanifest::logged().split_off(seen).iter().any(|line| line.contains("Trash changed; confirm again.")));
        let unknown = Session::default().handle(r#"{"c":"trashbrowse","op":"nothing","id":4}"#);
        assert!(unknown.contains(r#""stale":false,"error":"Unknown Trash request.""#), "{}", unknown);
    }
    #[test]
    fn a_review_record_that_does_not_read_back_says_the_trash_changed_and_logs_which() {
        let seen = crate::backend::trashmanifest::logged().len();
        let refused = record_item(r#"{"original":"/o"}"#).unwrap_err();
        assert_eq!(refused.shown(), "The Trash changed, so nothing was done. Try again.");
        assert!(crate::backend::trashmanifest::logged().split_off(seen).iter().any(|line| line.contains("Missing reviewed Trash URI.")));
        assert_eq!(String::from(record_text(vec![0xff]).unwrap_err()), "The Trash changed, so nothing was done. Try again.");
    }
    #[test]
    fn attributes_require_exact_names() {
        let text = "  standard::size: 12\n  id::file: abc\n";
        assert_eq!(attribute(text, "standard::size"), Some("12"));
        assert_eq!(attribute(text, "standard::siz"), None);
    }
    #[test]
    fn restore_selection_rejects_a_reused_uri_or_changed_metadata() {
        let original = Detail {
            item: Item { uri: "trash:///same.txt".into(), original: "/original/same.txt".into() },
            size: 8, deleted: "2026-09-08T10:00:00".into(), identity: "filesystem:inode-one".into(),
            directory: false, icon: "text-plain".into(), backing: None,
        };
        let observed = selection_identity(&original);
        assert!(require_selected_identity(&original, &observed).is_ok());
        assert!(require_selected_identity(&original, "").is_err());
        let mut replacement = original.clone();
        replacement.identity = "filesystem:inode-two".into();
        assert!(require_selected_identity(&replacement, &observed).is_err());
        replacement = original.clone();
        replacement.item.original = "/other/same.txt".into();
        assert!(require_selected_identity(&replacement, &observed).is_err());
        replacement = original.clone();
        replacement.size += 1;
        assert!(require_selected_identity(&replacement, &observed).is_err());
    }
    #[test]
    fn restore_selection_rejects_a_replaced_backing_inode() {
        use crate::backend::testdir::TestDir;
        use std::os::unix::fs::MetadataExt;
        let d = TestDir::new("trash-restore-identity");
        d.dir("files");
        d.dir("info");
        let path = d.file("files/same.txt", "original");
        d.file("info/same.txt.trashinfo", "[Trash Info]\nPath=/original/same.txt\n");
        let backing = || {
            let meta = path.symlink_metadata().unwrap();
            Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap()
        };
        let mut item = Detail {
            item: Item { uri: "trash:///same.txt".into(), original: "/original/same.txt".into() },
            size: 8, deleted: "2026-09-08T10:00:00".into(), identity: "provider-identity".into(),
            directory: false, icon: "text-plain".into(), backing: Some(backing()),
        };
        let observed = selection_identity(&item);
        std::fs::rename(&path, d.join("files/old.txt")).unwrap();
        std::fs::write(&path, "replaced").unwrap();
        item.backing = Some(backing());
        assert!(require_selected_identity(&item, &observed).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "replaced");
    }

    // Sample body: root ext4 beside one hung nfs mount, the same shape the slow tests map a sandbox onto.
    fn stub_body(dir: &std::path::Path) -> String {
        format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", dir.to_string_lossy())
    }

    // One sweep over restore and delete: each waits on a mount held slow and runs elsewhere.
    #[test]
    fn restore_and_delete_wait_on_a_pending_mount_and_run_elsewhere() {
        let remote = crate::backend::testdir::TestDir::new("trashgate");
        let body = stub_body(remote.path());
        let listing = format!("trash:///a.txt\t{}/a.txt\n", remote.path().to_string_lossy());
        let line = r#"{"c":"trashbrowse","op":"restore","id":9,"uris":["trash:///a.txt"],"identities":["held"]}"#;
        // A restore whose original sits on the held mount waits before gio runs.
        let mut held = Session {
            items: parse_list(&listing).unwrap(),
            scratch: remote.path().to_path_buf(),
            pending: vec![remote.path().to_path_buf()],
            body: body.clone(),
            ..Session::default()
        };
        let busy = held.handle(line);
        assert!(busy.contains(r#""ok":false"#) && busy.contains("already running"), "a restore onto a pending mount waits: {}", busy);
        // The same restore elsewhere runs past the gate to gio's own answer.
        let mut free = Session {
            items: parse_list(&listing).unwrap(),
            scratch: remote.path().to_path_buf(),
            ..Session::default()
        };
        let past = free.handle(line);
        assert!(!past.contains("already running"), "a restore elsewhere runs past the gate: {}", past);
        // A delete whose original sits on the held mount waits, and keeps its confirmation for the retry.
        let mut targets = Manifest::new(remote.path()).unwrap();
        targets.append(format!(r#"{{"uri":"trash:///a.txt","original":"{}/a.txt","identity":""}}"#,
            remote.path().to_string_lossy()).as_bytes()).unwrap();
        let confirmation = Confirmation { token: 7, items: Manifest::new(remote.path()).unwrap(),
            trees: Manifest::new(remote.path()).unwrap(), listing: Vec::new(), targets, count: 0, all: false };
        let delete = r#"{"c":"trashbrowse","op":"delete","id":9,"token":7}"#;
        let mut held = Session { confirmation: Some(confirmation), scratch: remote.path().to_path_buf(),
            pending: vec![remote.path().to_path_buf()], body, ..Session::default() };
        let busy = held.handle(delete);
        assert!(busy.contains(r#""ok":false"#) && busy.contains("already running"), "a delete onto a pending mount waits: {}", busy);
        assert!(held.confirmation.is_some(), "the refused delete keeps its confirmation for the retry");
        // The same delete elsewhere runs past the gate to the listing's own answer.
        let mut targets = Manifest::new(remote.path()).unwrap();
        targets.append(format!(r#"{{"uri":"trash:///a.txt","original":"{}/a.txt","identity":""}}"#,
            remote.path().to_string_lossy()).as_bytes()).unwrap();
        let confirmation = Confirmation { token: 7, items: Manifest::new(remote.path()).unwrap(),
            trees: Manifest::new(remote.path()).unwrap(), listing: Vec::new(), targets, count: 0, all: false };
        let mut free = Session { confirmation: Some(confirmation), scratch: remote.path().to_path_buf(), ..Session::default() };
        let past = free.handle(delete);
        assert!(!past.contains("already running"), "a delete elsewhere runs past the gate: {}", past);
    }
}
