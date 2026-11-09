// GIO owns the shared Trash store; this worker owns only a listing and a reviewed confirmation.
use crate::backend::opsreq::OpMsg;
use crate::backend::trashdelete::Reviewed;
use crate::backend::trashmanifest::{Cancellation, Manifest, Op, Outcome, Refusal};
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};
use flea::bytename::ExactText as _;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::{Duration, Instant};

mod job;
mod details;
#[cfg(test)]
mod queue_tests;
pub(crate) use job::request;
#[cfg(test)]
use job::delete_confirmed;

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
    nodes: usize,
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
    job: Option<job::Progress>,
    // Mounts the loop holds slow writes on, with the mount table they were read from, so restore and delete wait for them.
    pending: Vec<PathBuf>,
    body: String,
}
impl Default for Session {
    fn default() -> Self {
        Self { items: Vec::new(), confirmation: None, selection: None, next_token: 0,
            cancellation: Cancellation::default(), scratch: std::env::temp_dir(), job: None, pending: Vec::new(), body: String::new() }
    }
}

type BrowserRequest = (String, Cancellation, Vec<PathBuf>, String, Option<job::Progress>);
pub struct TrashBrowser {
    tx: Sender<BrowserRequest>,
    cancellation: Cancellation,
}
impl TrashBrowser {
    pub fn new(replies: Sender<OpMsg>) -> Self {
        let (tx, rx) = channel::<BrowserRequest>();
        thread::spawn(move || {
            let mut session = Session::default();
            for (request, cancellation, pending, body, job) in rx {
                session.cancellation = cancellation;
                session.pending = pending;
                session.body = body;
                session.job = job;
                if let Some(progress) = &session.job {
                    let count = session.confirmation.as_ref().filter(|c| Some(c.token) == field_usize(&request, "token")).map(|c| c.count).unwrap_or(0);
                    progress.count.set(count);
                    progress.nodes.set(session.confirmation.as_ref().filter(|c| Some(c.token) == field_usize(&request, "token")).map(|c| c.nodes).unwrap_or(0));
                    progress.sample("checking", 0, 0, None, true);
                }
                let line = session.handle(&request);
                let message = match session.job.take() {
                    Some(progress) => progress.finish(line, session.cancellation.check().is_err()),
                    None => OpMsg::Meta { line },
                };
                if replies.send(message).is_err() { break; }
            }
        });
        Self { tx, cancellation: Cancellation::default() }
    }
}
impl Drop for TrashBrowser {
    fn drop(&mut self) { self.cancellation.next(); }
}

fn gio(args: &[&str]) -> Result<String, Refusal> {
    let prefix = super::trash::gio_prefix();
    answer(crate::backend::locale::command(&prefix[0])
        .args(&prefix[1..])
        .args(flea::bytename::args(args))
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

// Sample input, gio list -u -h -a trash::orig-path trash:///: "trash:///a%0Ab.txt\t4\t(regular)\ttrash::orig-path=/tmp/a\\x0ab.txt\n".
// gio escapes every control, non-ASCII and backslash byte, so one line is one item, and an item it cannot describe is left out, never the rest.
fn parse_list(text: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let mut left_out = Vec::new();
    let mut uris = Vec::new();
    for line in text.lines() {
        let mut fields = line.split('\t');
        let uri = fields.next().unwrap_or_default();
        uris.push(uri);
        let original = fields
            .find_map(|field| field.strip_prefix("trash::orig-path="))
            .and_then(unescape);
        match original {
            Some(original)
                if uri.starts_with("trash:///")
                    && uri.len() > "trash:///".len()
                    && original.starts_with('/') =>
            {
                items.push(Item {
                    uri: uri.into(),
                    original,
                })
            }
            _ => left_out.push(uri.to_string()),
        }
    }
    items.sort_by(|a, b| a.uri.cmp(&b.uri));
    uris.sort_unstable();
    // Two lines under one uri, either one left out or not, leave no way to tell which entry an action would reach, so neither is offered.
    let doubled: HashSet<String> = uris
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .map(|pair| pair[0].to_string())
        .collect();
    items.retain(|item| !doubled.contains(&item.uri));
    left_out.retain(|uri| !doubled.contains(uri));
    left_out.extend(doubled);
    if !left_out.is_empty() {
        crate::backend::trashmanifest::log(&format!(
            "Trash left out what gio could not describe: {}",
            left_out.join(", ")
        ));
    }
    items
}
// Sample input: "/tmp/a\\x0ab.txt" is the original "/tmp/a\nb.txt"; None for a broken escape or an original that is not UTF-8.
fn unescape(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut rest = value.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;
        if byte == b'\\' {
            let hex = rest.strip_prefix(b"x").and_then(|after| after.get(..2))?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            rest = &rest[3..];
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok()
}
// The gio list line for each "uri<TAB>original" pair, so a test names its items the way they read.
#[cfg(test)]
fn listed(pairs: &str) -> Vec<Item> {
    parse_list(
        &pairs
            .lines()
            .map(|line| line.replacen('\t', "\t1\t(regular)\ttrash::orig-path=", 1) + "\n")
            .collect::<String>(),
    )
}
fn list() -> Result<Vec<Item>, Refusal> {
    list_with(gio)
}

// The listing over any reader of gio's text, so a test can feed the two outputs through the real join.
fn list_with(read: impl Fn(&[&str]) -> Result<String, Refusal>) -> Result<Vec<Item>, Refusal> {
    let mut items = parse_list(&read(&["list", "-u", "-h", "-a", "trash::orig-path", "trash:///"])?);
    // One gio call dates every volume's items, a dot name too; a failed call leaves the order by uri.
    let dates = parse_dates(&read(&["list", "-u", "-h", "-a", "trash::deletion-date", "trash:///"]).unwrap_or_default());
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
    detail_text(item, &text)
}
fn detail_text(item: &Item, text: &str) -> Result<Detail, Refusal> {
    let size = attribute(text, "standard::size")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| unreadable("The Trash helper did not report the item's size."))?;
    let identity = match (
        attribute(text, "id::filesystem"),
        attribute(text, "id::file"),
    ) {
        (Some(fs), Some(file)) if !fs.is_empty() && !file.is_empty() => format!("{}:{}", fs, file),
        _ => String::new(),
    };
    let backing = attribute(text, "standard::target-uri")
        .and_then(|uri| uri.strip_prefix("file://"))
        .and_then(|path| {
            Reviewed::inspect(
                PathBuf::from(<std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(crate::paths::percent_decode_bytes(path))),
                attribute(text, "id::file").unwrap_or(""),
            )
            .ok()
        });
    Ok(Detail {
        backing,
        item: item.clone(),
        size,
        deleted: attribute(text, "trash::deletion-date")
            .unwrap_or("")
            .into(),
        identity,
        directory: attribute(text, "standard::type") == Some("2"),
        icon: attribute(text, "standard::icon")
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

fn thumbnail(item: &Detail) -> String {
    if !item.directory && item.icon.starts_with("image-") {
        if let Some(backing) = &item.backing {
            if let Some((path, mtime)) = backing.thumbnail() {
                return format!(r#"{{"path":"{}","provider":"{}","identity":"{}","mtime":{}}}"#,
                    escape(&path.exact_text()), escape(&backing.provider_identity()),
                    escape(&backing.selection_identity()), mtime);
            }
        }
    }
    "null".into()
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
// Purge once on request exit, including a partial restore that stops on an unreadable target record.
struct ThumbnailPurge(Vec<String>);
impl ThumbnailPurge {
    fn record(&mut self, reviewed: &Reviewed, result: &Result<(), Refusal>) {
        if let Some(uri) = reviewed.departed_uri(result) { self.0.push(uri); }
    }
}
impl Drop for ThumbnailPurge {
    fn drop(&mut self) { super::trashthumbcache::purge(&self.0); }
}
#[cfg(test)]
fn delete_one(item: &Detail, recovery: &Result<PathBuf, String>, thumbnails: &mut ThumbnailPurge) -> Result<(), String> {
    let reviewed = item.backing.as_ref().ok_or_else(|| unchecked(Op::Delete).shown())?;
    let result = reviewed.delete(recovery.as_ref().map_err(Clone::clone)?);
    thumbnails.record(reviewed, &result);
    result.map_err(String::from)
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
            "delete" => self.delete(line),
            _ => Err("Unknown Trash request.".into()),
        }
    }
    fn window(&mut self, start: usize, count: usize) -> Result<String, String> {
        let start = start.min(self.items.len().saturating_sub(1));
        self.cancellation.check()?;
        // One gio call reads the whole window, in order; any item it cannot read fails the window as before.
        let window = &self.items[start..self.items.len().min(start + count.min(350))];
        let rows = if window.is_empty() { Vec::new() } else { details::many(window)? };
        self.cancellation.check()?;
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
        let mut classes = HashMap::new();
        let rows = rows.iter().map(|item| {
            let mut text = row(item);
            text.pop();
            let class = if self.body.is_empty() { "null".into() } else {
                let class = item.backing.as_ref().and_then(Reviewed::thumbnail_path).and_then(|path| path.parent().map(flea::bytename::exact))
                    .map(|parent| *classes.entry(parent.clone()).or_insert_with(|| super::extclass::classify_in(&parent, &self.body))).unwrap_or("");
                format!(r#""{}""#, class)
            };
            text.push_str(&format!(r#","bulkSelected":{},"storageClass":{},"thumbnail":{},{}}}"#, members.contains(&item.item.uri), class, thumbnail(item), details::content(item)));
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
        if self.pending.is_empty() { return Ok(false); }
        let mut offset = 0;
        while let Some(bytes) = targets.records().next(&mut offset)? {
            self.cancellation.check()?;
            let item = record_item(&record_text(bytes)?)?;
            let mount = crate::backend::iomount::mount_key(&flea::bytename::exact(&item.original), &self.body);
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
        let mut nodes = 0usize;
        let mut bytes = 0u64;
        loop {
            let batch = details::records(&targets, &mut cursor)?;
            if batch.is_empty() { break; }
            self.cancellation.check()?;
            let names = batch.iter().map(|text| record_item(text)).collect::<Result<Vec<_>, _>>()?;
            let current_items = details::many(&names)?;
            for (text, mut current) in batch.into_iter().zip(current_items) {
                self.cancellation.check()?;
                if !all && refresh_token == 0 {
                    require_selected_identity(&current, &field_str(&text, "identity").unwrap_or_default())?;
                }
                if current.identity.is_empty() { return Err(indistinct(false).into()); }
                let size = current.backing.as_mut()
                    .ok_or_else(|| unchecked(Op::Delete))?
                    .snapshot(&mut trees, &self.scratch, &self.cancellation)?;
                nodes = nodes.checked_add(current.backing.as_ref().unwrap().reviewed_nodes()).ok_or("Trash file total exceeds the supported range.")?;
                bytes = bytes.checked_add(size).ok_or("Trash byte total exceeds the supported range.")?;
                items.append(save_detail(&current)?.as_bytes())?;
                count += 1;
            }
        }
        self.next_token += 1;
        self.confirmation = Some(Confirmation { token: self.next_token, items, trees, listing: self.items.clone(), targets, count, nodes, all });
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
        self.cancellation.check()?;
        if !same_items(&list()?, &confirmation.listing) { return Ok(false); }
        let mut cursor = 0;
        let mut checked = 0;
        loop {
            self.cancellation.check()?;
            let batch = details::records(&confirmation.items, &mut cursor)?;
            if batch.is_empty() { break; }
            let old_items = batch.iter().map(|text| load_detail(text, &confirmation.trees)).collect::<Result<Vec<_>, _>>()?;
            let names = old_items.iter().map(|old| old.item.clone()).collect::<Vec<_>>();
            let current_items = details::many(&names)?;
            for (old, current) in old_items.iter().zip(current_items) {
                self.cancellation.check()?;
                let Some(backing) = old.backing.as_ref() else { return Ok(false); };
                if !backing.contents_unchanged(&self.cancellation)? { return Ok(false); }
                if !selection_current(&current, &selection_identity(old)) { return Ok(false); }
                checked += 1;
                if let Some(progress) = &self.job { progress.sample("checking", 0, checked, Some(old), false); }
            }
        }
        Ok(true)
    }
    fn restore(&mut self, line: &str) -> Result<String, String> {
        let mut thumbnails = ThumbnailPurge(Vec::new());
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
                let backing = current.backing.as_ref().ok_or_else(|| unchecked(Op::Restore))?;
                let result = backing.restore(&flea::bytename::exact(&item.original), &crate::backend::trashdelete::recovery_root()?);
                thumbnails.record(backing, &result);
                let result = result.map_err(String::from);
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
#[path = "trashbrowse/thumbnail_tests.rs"]
mod thumbnail_tests;
#[cfg(test)]
#[path = "trashbrowse/tests.rs"]
mod tests;
