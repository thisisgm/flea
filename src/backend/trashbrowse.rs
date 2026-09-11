// GIO owns the shared Trash store; this worker owns only a listing and a reviewed confirmation.
use crate::backend::opsreq::OpMsg;
use crate::backend::trashdelete::Reviewed;
use crate::backend::trashmanifest::{Cancellation, Manifest};
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::{Duration, Instant};

const SUMMARY_BUDGET_MS: u64 = 2000;
const GIO_TIMEOUT: &str = "10s";
const STALE_CONFIRMATION: &str = "Trash changed; review a fresh confirmation.";

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
}
impl Default for Session {
    fn default() -> Self {
        Self { items: Vec::new(), confirmation: None, selection: None, next_token: 0,
            cancellation: Cancellation::default(), scratch: std::env::temp_dir() }
    }
}

pub struct TrashBrowser {
    tx: Sender<(String, Cancellation)>,
    cancellation: Cancellation,
}
impl TrashBrowser {
    pub fn new(replies: Sender<OpMsg>) -> Self {
        let (tx, rx) = channel::<(String, Cancellation)>();
        thread::spawn(move || {
            let mut session = Session::default();
            for (request, cancellation) in rx {
                session.cancellation = cancellation;
                let line = session.handle(&request);
                if replies.send(OpMsg::Meta { line }).is_err() { break; }
            }
        });
        Self { tx, cancellation: Cancellation::default() }
    }
    pub fn request(&self, line: String) {
        let _ = self.tx.send((line, self.cancellation.next()));
    }
}
impl Drop for TrashBrowser {
    fn drop(&mut self) { self.cancellation.next(); }
}

fn gio(args: &[&str]) -> Result<String, String> {
    let output = Command::new("timeout")
        .env("LC_ALL", "C")
        .args(["--kill-after=1s", GIO_TIMEOUT, "gio"])
        .args(args)
        .output()
        .map_err(|e| format!("Could not run the Trash helper: {}", e))?;
    if !output.status.success() {
        if matches!(output.status.code(), Some(124 | 137)) {
            return Err("The GIO Trash request timed out.".into());
        }
        return Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap_or("gio failed")
            .to_string());
    }
    String::from_utf8(output.stdout).map_err(|_| "GIO returned invalid text.".into())
}

// Sample input: "trash:///a.txt\t/home/gm/a.txt\n"; malformed framing must not silently lose an item.
fn parse_list(text: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    for line in text.lines() {
        let (uri, original) = line
            .split_once('\t')
            .ok_or("GIO returned an unreadable Trash listing.")?;
        if !uri.starts_with("trash:///")
            || uri.len() <= "trash:///".len()
            || !original.starts_with('/')
        {
            return Err("GIO returned an invalid Trash identity.".into());
        }
        items.push(Item {
            uri: uri.into(),
            original: original.into(),
        });
    }
    items.sort_by(|a, b| a.uri.cmp(&b.uri));
    if items.windows(2).any(|pair| pair[0].uri == pair[1].uri) {
        return Err("GIO returned duplicate Trash identities.".into());
    }
    Ok(items)
}
fn list() -> Result<Vec<Item>, String> {
    parse_list(&gio(&["trash", "--list"])?)
}

// Sample input, gio info: "  standard::size: 123\n  trash::deletion-date: 2026-09-08T10:00:00".
fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        line.trim_start()
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix(": "))
    })
}
fn detail(item: &Item) -> Result<Detail, String> {
    let text = gio(&["info", "--nofollow-symlinks", "--attributes=standard::size,standard::type,trash::deletion-date,id::file,id::filesystem,standard::icon,standard::target-uri", &item.uri])?;
    let size = attribute(&text, "standard::size")
        .and_then(|s| s.parse().ok())
        .ok_or("GIO did not report Trash item size.")?;
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

fn require_selected_identity(item: &Detail, expected: &str) -> Result<(), String> {
    if expected.is_empty() || item.identity.is_empty() || selection_identity(item) != expected {
        return Err("Trash selection changed; select the item again before acting on it.".into());
    }
    Ok(())
}

fn record_text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| "Invalid text in Trash review.".into())
}
fn record_item(text: &str) -> Result<Item, String> {
    Ok(Item { uri: field_str(text, "uri").ok_or("Missing reviewed Trash URI.")?,
        original: field_str(text, "original").ok_or("Missing reviewed original path.")? })
}
fn save_detail(item: &Detail) -> Result<String, String> {
    let mut text = row(item);
    text.pop();
    text.push_str(&format!(r#","providerIdentity":"{}","backing":"{}"}}"#,
        escape(&item.identity), escape(&item.backing.as_ref().ok_or("Missing Trash backing review.")?.saved()?)));
    Ok(text)
}
fn load_detail(text: &str, trees: &Manifest) -> Result<Detail, String> {
    Ok(Detail {
        item: record_item(text)?,
        size: field_usize(text, "size").ok_or("Missing reviewed Trash size.")? as u64,
        deleted: field_str(text, "deleted").unwrap_or_default(),
        identity: field_str(text, "providerIdentity").ok_or("Missing reviewed provider identity.")?,
        directory: field_bool(text, "directory"),
        icon: field_str(text, "icon").unwrap_or_default(),
        backing: Some(Reviewed::from_saved(&field_str(text, "backing").ok_or("Missing Trash backing review.")?, trees)?),
    })
}
fn failure(item: &Item, expected: &str, error: &str) -> String {
    format!(r#"{{"uri":"{}","name":"{}","identity":"{}","error":"{}"}}"#,
        escape(&item.uri), escape(item.original.rsplit('/').next().unwrap_or(&item.original)), escape(expected), escape(error))
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
            Err(error) => format!(
                r#"{{"t":"trashbrowse","id":{},"op":"{}","ok":false,"stale":{},"error":"{}"}}"#,
                id,
                escape(&op),
                op == "delete" && error == STALE_CONFIRMATION,
                escape(&error)
            ),
        }
    }
    fn execute(&mut self, op: &str, line: &str) -> Result<String, String> {
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
            "summary" => self.summary(),
            "select" => self.select_all(),
            "window" => self.window(
                field_usize(line, "start").unwrap_or(0),
                field_usize(line, "count").unwrap_or(100),
            ),
            "prepare" => self.prepare(line),
            "check" => {
                let valid = self.valid(field_usize(line, "token").unwrap_or(0))?;
                Ok(format!(r#""valid":{}"#, valid))
            }
            "cancel" => {
                self.confirmation = None;
                if field_bool(line, "clearSelection") { self.selection = None; }
                Ok(r#""cancelled":true"#.into())
            }
            "restore" => self.restore(line),
            "delete" => {
                if !self.valid(field_usize(line, "token").unwrap_or(0))? {
                    return Err(STALE_CONFIRMATION.into());
                }
                let confirmation = self
                    .confirmation
                    .take()
                    .ok_or("Trash confirmation expired.")?;
                let mut done = 0;
                let mut failures = Vec::new();
                let mut offset = 0;
                while let Some(bytes) = confirmation.items.records().next(&mut offset)? {
                    let item = load_detail(&record_text(bytes)?, &confirmation.trees)?;
                    match item.backing.as_ref().ok_or("Trash backing identity is unavailable.".to_string()).and_then(|reviewed| reviewed.delete(&crate::backend::trashdelete::recovery_root()?)) {
                        Ok(()) => done += 1,
                        Err(error) => failures.push(failure(&item.item, &surviving_identity(&item), &error)),
                    }
                }
                Ok(format!(
                    r#""done":{},"failed":{},"failures":[{}]"#,
                    done,
                    failures.len(),
                    failures.join(",")
                ))
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
            let mut offset = 0;
            while let Some(bytes) = selection.items.records().next(&mut offset)? {
                self.cancellation.check()?;
                let text = record_text(bytes)?;
                let item = record_item(&text)?;
                if self.items.binary_search_by(|current| current.uri.cmp(&item.uri)).is_err() { continue; }
                selected_count += 1;
                if let Some(current) = visible.get(&item.uri) {
                    let expected = field_str(&text, "identity").unwrap_or_default();
                    if require_selected_identity(current, &expected).is_err() { stale = true; }
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
                return Err("The Trash provider did not report stable item identities; selection is unavailable.".into());
            }
            records.append(row(&current).as_bytes())?;
        }
        if list()? != self.items { return Err("Trash changed while selecting its contents; select again.".into()); }
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
            return Err("Trash selection has no reviewed identity; select the item again.".into());
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
                .ok_or("Trash selection expired; select the items again.")?;
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
                    .ok_or("Trash selection expired; refresh the view.")?;
                if expected.is_empty() { return Err("Trash selection has no reviewed identity; select the item again.".into()); }
                targets.append(format!(r#"{{"uri":"{}","original":"{}","identity":"{}"}}"#,
                    escape(uri), escape(&item.original), escape(expected)).as_bytes())?;
            }
        }
        if targets.len() == 0 { return Err(if all { "Trash is empty." } else { "Select at least one Trash item." }.into()); }
        Ok(targets)
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
            if current.identity.is_empty() { return Err("The Trash provider did not report a stable item identity.".into()); }
            let size = current.backing.as_mut().ok_or("Trash backing identity is unavailable; deletion is unavailable.")?
                .snapshot(&mut trees, &self.scratch, &self.cancellation)?;
            bytes = bytes.checked_add(size).ok_or("Trash byte total exceeds the supported range.")?;
            items.append(save_detail(&current)?.as_bytes())?;
            count += 1;
        }
        self.next_token += 1;
        self.confirmation = Some(Confirmation { token: self.next_token, items, trees, listing: self.items.clone(), targets, all });
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
        if list()? != confirmation.listing { return Ok(false); }
        let mut cursor = 0;
        while let Some(bytes) = confirmation.items.records().next(&mut cursor)? {
            self.cancellation.check()?;
            let old = load_detail(&record_text(bytes)?, &confirmation.trees)?;
            let Some(backing) = old.backing.as_ref() else { return Ok(false); };
            if !backing.contents_unchanged(&self.cancellation)? { return Ok(false); }
            let current = detail(&old.item)?;
            if require_selected_identity(&current, &selection_identity(&old)).is_err() { return Ok(false); }
        }
        Ok(true)
    }
    fn restore(&mut self, line: &str) -> Result<String, String> {
        if field_bool(line, "all") { self.items = list()?; }
        let targets = self.targets(line)?;
        let mut done = 0;
        let mut failures = Vec::new();
        let mut cursor = 0;
        while let Some(bytes) = targets.records().next(&mut cursor)? {
            let text = record_text(bytes)?;
            let item = record_item(&text)?;
            let mut retained = field_str(&text, "identity").unwrap_or_default();
            let restored = detail(&item).and_then(|current| {
                if !field_bool(line, "all") {
                    require_selected_identity(&current, &field_str(&text, "identity").unwrap_or_default())?;
                }
                let result = current.backing.as_ref().ok_or("Trash backing identity is unavailable; restore is unavailable.")?
                    .restore(&PathBuf::from(&item.original), &crate::backend::trashdelete::recovery_root()?);
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
mod tests {
    use super::*;
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
            trees: Manifest::new(d.path()).unwrap(), listing: Vec::new(), targets, all: false };
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
}
