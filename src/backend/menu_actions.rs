// Menu snapshots own selected identities; registry work runs only after an explicit menu action.
use crate::backend::opsreq::OpMsg;
use super::menu_registry::{self, Registry};
use super::trashmanifest::Cancellation;
use crate::json::{escape, field_bool, field_str, field_usize};
use std::fs::{Metadata, OpenOptions};
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct MenuActions {
    requests: SyncSender<(String, Vec<String>, Option<String>, Cancellation)>,
    replies: Sender<OpMsg>,
    snapshot: Arc<Mutex<Snapshot>>,
    cancellation: Mutex<Cancellation>,
    registry: Registry,
    requested_id: AtomicUsize,
    restoration: Arc<Mutex<(usize, Vec<Selected>)>>,
}
impl MenuActions {
    pub fn new(replies: Sender<OpMsg>) -> Self {
        // One pending request bounds repeated activation while an application registry query runs.
        let (requests, receiver) = sync_channel::<(String, Vec<String>, Option<String>, Cancellation)>(1);
        let output = replies.clone();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let published = Arc::clone(&snapshot);
        let registry = Registry::default();
        let queries = registry.clone();
        let restoration = Arc::new(Mutex::new((0, Vec::new())));
        let completed = Arc::clone(&restoration);
        std::thread::spawn(move || {
            while let Ok((line, paths, cursor, cancel)) = receiver.recv() {
                let mut state = published.lock().unwrap().clone();
                let mut reply = if cancel.check().is_ok() {
                    state.handle_request(&line, paths, cursor.as_deref(), &queries, &cancel)
                } else { response(&line, Err("Menu request cancelled.".into())) };
                if cancel.check().is_err() && field_str(&line, "op").as_deref() != Some("delete") {
                    reply.insert_str(reply.len() - 1, r#", "cancelled":true"#);
                }
                if field_str(&line, "op").as_deref() == Some("delete") {
                    // Close expires mutation immediately; the completed operation still identifies survivors for reselection.
                    *completed.lock().unwrap() = (state.id, state.items.clone());
                }
                if matches!(field_str(&line, "op").as_deref(), Some("snapshot" | "close" | "prepareDelete" | "refreshDelete" | "delete" | "providerDestination" | "activate")) {
                    publish_snapshot(published.lock().unwrap(), state, &cancel);
                }
                let message = if field_str(&line, "op").as_deref() == Some("delete") {
                    OpMsg::MenuDeleteDone { line: reply }
                } else { OpMsg::Meta { line: reply } };
                if output.send(message).is_err() { break; }
            }
        });
        Self { requests, replies, snapshot, cancellation: Mutex::new(Cancellation::default()), registry, requested_id: AtomicUsize::new(0), restoration }
    }
    pub(crate) fn retain_survivors(&self, id: usize, matches: &mut Vec<(&str, usize)>) -> Result<(), String> {
        let restoration = self.restoration.lock().map_err(|_| "The menu service stopped; refresh this window.")?;
        if id == 0 || restoration.0 != id { return Err("Deletion survivor identities expired; select the items again.".into()); }
        let originals: HashMap<_, _> = restoration.1.iter().map(|item| (item.path.as_path(), item)).collect();
        matches.retain(|(path, _)| originals.get(Path::new(path)).is_some_and(|item| item.current().is_ok()));
        Ok(())
    }
    pub(crate) fn selection(&self, id: usize) -> Result<Vec<Selected>, String> {
        let snapshot = self.snapshot.lock().map_err(|_| "The menu service stopped; reopen this window.")?;
        if id == 0 || snapshot.id != id || snapshot.items.is_empty() {
            return Err("Menu selection expired; reopen the menu.".into());
        }
        Ok(snapshot.items.clone())
    }
    pub(crate) fn provider_destination(&self, id: usize, dest: &Path) -> Result<Option<Selected>, String> {
        let snapshot = self.snapshot.lock().map_err(|_| "The menu service stopped; reopen this window.")?;
        if snapshot.id != id { return Err("Menu selection expired; reopen the menu.".into()); }
        if snapshot.provider_transfer {
            snapshot.check_provider_destination(dest)?;
            return Ok(snapshot.provider_destination.clone());
        }
        Ok(None)
    }
    pub(crate) fn selected_path(&self, id: usize, path: &Path) -> Result<Selected, String> {
        let snapshot = self.snapshot.lock().map_err(|_| "The menu service stopped; reopen this window.")?;
        if id == 0 || snapshot.id != id { return Err("Menu selection expired; reopen the menu.".into()); }
        snapshot.items.iter().find(|item| item.path == path).cloned()
            .ok_or_else(|| "This path was not in the menu selection; reopen the menu.".into())
    }
    pub fn request(&self, line: String, paths: Vec<String>, cursor: Option<String>) -> bool {
        let op = field_str(&line, "op").unwrap_or_default();
        let mut cancellation = self.cancellation.lock().unwrap();
        if op == "snapshot" || op == "close" {
            let id = field_usize(&line, "id").unwrap_or(0);
            if op == "close" && self.requested_id.load(Ordering::Relaxed) != id {
                let _ = self.replies.send(OpMsg::Meta { line: response(&line, Ok(String::new())) });
                return false;
            }
            *cancellation = cancellation.next();
            self.requested_id.store(if op == "snapshot" { id } else { 0 }, Ordering::Relaxed);
            *self.snapshot.lock().unwrap() = Snapshot::default();
            if let Err(error) = self.registry.cancel() {
                let _ = self.replies.send(OpMsg::Meta { line: response(&line, Err(error)) });
                return false;
            }
            if op == "close" {
                let _ = self.replies.send(OpMsg::Meta { line: response(&line, Ok(String::new())) });
                return false;
            }
        }
        if let Err(error) = self.requests.try_send((line, paths, cursor, cancellation.clone())) {
            let (request, reason) = match error {
                TrySendError::Full(request) => (request, "A menu request is still running; try again when it finishes."),
                TrySendError::Disconnected(request) => (request, "The menu service stopped; reopen this window."),
            };
            let reply = response(&request.0, Err(reason.into()));
            let _ = self.replies.send(OpMsg::Meta { line: reply });
            return false;
        }
        true
    }
}

impl Drop for MenuActions {
    fn drop(&mut self) {
        self.cancellation.lock().unwrap().next();
        if let Err(error) = self.registry.cancel() { eprintln!("flea: {}", error); }
    }
}

#[derive(Clone, Default)]
struct Snapshot {
    id: usize,
    items: Vec<Selected>,
    cursor: Option<Selected>,
    deletion: Option<Arc<super::menudelete::Review>>,
    provider_destination: Option<Selected>,
    provider_transfer: bool,
}

// The lock must already be held when cancellation is checked, or close can be followed by a stale publication.
fn publish_snapshot(mut published: MutexGuard<'_, Snapshot>, state: Snapshot, cancel: &Cancellation) {
    if cancel.check().is_ok() { *published = state; }
}
#[derive(Clone)]
pub(crate) struct Selected {
    pub path: PathBuf,
    dev: u64,
    ino: u64,
    kind: u32,
}
impl Selected {
    pub(crate) fn inspect(path: &str) -> Result<Self, String> {
        let path = PathBuf::from(path);
        if !path.is_absolute() || path.file_name().is_none() {
            return Err("Menu selection requires an absolute item path.".into());
        }
        let meta = path.symlink_metadata().map_err(|e| format!("Could not inspect selected item {}: {}.", path.display(), crate::error::io_message(&e)))?;
        Ok(Self { path, dev: meta.dev(), ino: meta.ino(), kind: meta.mode() & 0o170000 })
    }
    pub(crate) fn current(&self) -> Result<Metadata, String> {
        let meta = self.path.symlink_metadata().map_err(|_| "Selected item moved or disappeared; reopen the menu.")?;
        if meta.dev() != self.dev || meta.ino() != self.ino || meta.mode() & 0o170000 != self.kind {
            return Err("Selected item changed; reopen the menu.".into());
        }
        Ok(meta)
    }
}

pub(crate) fn validate_sources(items: Option<&[Selected]>, paths: &[PathBuf]) -> Result<(), String> {
    let Some(items) = items else { return Ok(()); };
    let captured: HashSet<_> = items.iter().map(|item| item.path.as_path()).collect();
    let requested: HashSet<_> = paths.iter().map(|path| path.as_path()).collect();
    if captured != requested { return Err("The operation does not match the captured menu selection.".into()); }
    for item in items { item.current().map_err(|error| format!("{}: {}", item.path.display(), error))?; }
    Ok(())
}
impl Snapshot {
    // Sample input: {"c":"menuaction","op":"snapshot","id":3}; paths are resolved from the active listing by run.rs.
    fn handle_request(&mut self, line: &str, paths: Vec<String>, cursor: Option<&str>, registry: &Registry, cancel: &Cancellation) -> String {
        let id = field_usize(line, "id").unwrap_or(0);
        let op = field_str(line, "op").unwrap_or_default();
        let result = if op == "snapshot" {
            self.id = 0;
            self.items.clear();
            self.cursor = None;
            self.deletion = None;
            self.provider_destination = None;
            self.provider_transfer = false;
            if id == 0 || paths.is_empty() {
                Err("There are no selected items to inspect.".into())
            } else {
                paths.iter().map(|path| Selected::inspect(path)).collect::<Result<Vec<_>, _>>().and_then(|items| {
                    self.cursor = cursor.map(Selected::inspect).transpose()?;
                    self.items = items;
                    self.id = id;
                    Ok(format!(r#""count":{}"#, self.items.len()))
                })
            }
        } else if op == "close" {
            if self.id == id { *self = Self::default(); }
            Ok(String::new())
        } else {
            self.perform(id, &op, line, registry, cancel)
        };
        response(line, result)
    }
    fn perform(&mut self, id: usize, op: &str, line: &str, registry: &Registry, cancel: &Cancellation) -> Result<String, String> {
        if id == 0 || id != self.id || self.items.is_empty() {
            return Err("Menu selection expired; reopen the menu.".into());
        }
        if op == "delete" {
            let review = self.take_deletion(field_usize(line, "token").unwrap_or(0))?;
            let recovery = super::trashdelete::recovery_root()?;
            return match review.delete(&recovery, cancel) {
                Ok(result) => Ok(result),
                Err(error) => Ok(format!(r#""stale":true,"error":"{}""#, escape(&error))),
            }
        }
        if op == "checkDelete" {
            let token = field_usize(line, "token").unwrap_or(0);
            let review = self.deletion.as_ref().filter(|review| token > 0 && review.token == token)
                .ok_or("Deletion confirmation expired; review a fresh confirmation.")?;
            return Ok(format!(r#""valid":{}"#, review.validate(cancel).is_ok()));
        }
        if op == "refreshDelete" {
            self.deletion = None;
            let mut current = Vec::new();
            for item in &self.items {
                cancel.check()?;
                match item.path.symlink_metadata() {
                    Ok(_) => current.push(Selected::inspect(&item.path.to_string_lossy())?),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(format!("Could not inspect {}: {}.", item.path.display(), error)),
                }
            }
            self.items = current;
        }
        if op == "prepareDelete" { self.deletion = None; }
        for item in &self.items { item.current()?; }
        if op == "providerDestination" {
            let dest = field_str(line, "dest").unwrap_or_default();
            let selected = Selected::inspect(&dest)?;
            if !selected.current()?.is_dir() { return Err("Dropbox account folder is not a directory.".into()); }
            self.provider_destination = Some(selected);
            return Ok(String::new());
        }
        if op == "prepareDelete" || op == "refreshDelete" {
            let review = super::menudelete::Review::prepare(&self.items, &std::env::temp_dir(), cancel)?;
            let reply = format!(r#""token":{},"count":{},"bytes":{}"#, review.token, review.count, review.bytes);
            self.deletion = Some(Arc::new(review));
            return Ok(reply);
        }
        if op == "validate" || op == "activate" {
            let action = field_str(line, "action").unwrap_or_default();
            if action == "dropbox" || action == "sharelink" {
                self.check_provider_destination(Path::new(&field_str(line, "dest").unwrap_or_default()))?;
            }
            if op == "activate" { self.provider_transfer = action == "dropbox"; }
            if action.starts_with("taildrop:") || action == "sharelink" {
                let cursor = self.cursor.as_ref().ok_or("Cursor source was not captured; reopen the menu.")?;
                cursor.current()?;
                return Ok(format!(r#""action":"{}","paths":["{}"]"#, escape(&action), escape(&cursor.path.to_string_lossy())));
            }
            let needs_paths = op == "validate" || matches!(action.as_str(), "copy" | "cut" | "copypath" | "addFavourite") || action.starts_with("compress:");
            let paths: Vec<String> = if needs_paths { self.items.iter().map(|i| format!(r#""{}""#, escape(&i.path.to_string_lossy()))).collect() } else { Vec::new() };
            return Ok(format!(r#""action":"{}","paths":[{}],"dest":"{}""#,
                escape(&action), paths.join(","),
                escape(&field_str(line, "dest").unwrap_or_default())));
        }
        if self.items.len() != 1 { return Err("This action requires one selected item.".into()); }
        let item = &self.items[0];
        let meta = item.current()?;
        match op {
            "properties" => {
                let kind = if meta.file_type().is_symlink() { "Symbolic link" } else if meta.is_dir() { "Directory" }
                           else if meta.is_file() { "File" } else { "Special file" };
                let target = if meta.file_type().is_symlink() {
                    std::fs::read_link(&item.path).map_err(|e| format!("Could not read symlink: {}.", e))?.to_string_lossy().to_string()
                } else { String::new() };
                Ok(format!(r#""path":"{}","kind":"{}","directory":{},"symlink":{},"target":"{}","bytes":{},"modified":{},"mode":"{:04o}","owner":"{}","uid":{},"gid":{}"#,
                    escape(&item.path.to_string_lossy()), kind, meta.is_dir(), meta.file_type().is_symlink(), escape(&target),
                    meta.len(), meta.mtime(), meta.mode() & 0o7777, escape(&super::owner::name(meta.uid())), meta.uid(), meta.gid()))
            }
            "applications" => {
                let found = menu_registry::catalogue(registry, &item.path, field_bool(line, "installed"), cancel)?;
                Ok(format!(r#""applications":[{}],"installed":[{}],"mime":"{}","kind":"{}","path":"{}""#,
                    application_entries(&found.handlers), application_entries(&found.installed),
                    escape(&found.mime), escape(&found.kind), escape(&item.path.to_string_lossy())))
            }
            "openWith" => {
                let app = menu_registry::resolve(&field_str(line, "application").unwrap_or_default(), cancel)?;
                item.current()?;
                registry.launch(&app.path, &item.path, cancel)?;
                // OpenWith.html rule 1: the dialog is the one place a default is written, and it is
                // written after the launch so a launcher that refuses outright leaves nothing behind.
                // The file is already open by this point, so a failure here says only what failed.
                if field_bool(line, "always") {
                    let mime = menu_registry::content_type(registry, &item.path, cancel)?;
                    // A directory and an unresolvable link both type as inode/*, and making an editor
                    // the desktop's default folder handler is not what ticking this box asks for.
                    if mime.starts_with("inode/") {
                        return Err(format!("Opened it, but {} has no file type to set a default for.", item.path.display()));
                    }
                    if let Err(error) = menu_registry::set_default(registry, &mime, &app.id, cancel) {
                        return Err(format!("Opened it, but the default was not saved: {}", error));
                    }
                }
                Ok(format!(r#""path":"{}""#, escape(&item.path.to_string_lossy())))
            }
            _ => Err("Unknown menu operation.".into()),
        }
    }
    fn take_deletion(&mut self, token: usize) -> Result<Arc<super::menudelete::Review>, String> {
        self.deletion.take().filter(|review| token > 0 && review.token == token)
            .ok_or_else(|| "Deletion confirmation expired; review a fresh confirmation.".into())
    }
    fn check_provider_destination(&self, dest: &Path) -> Result<(), String> {
        let selected = self.provider_destination.as_ref().filter(|selected| selected.path == dest)
            .ok_or("Dropbox account folder changed; reopen the menu.")?;
        selected.current().map_err(|_| "Dropbox account folder changed or disappeared; reopen the menu.")?;
        Ok(())
    }
    #[cfg(test)]
    fn handle(&mut self, line: &str, paths: Vec<String>) -> String {
        self.handle_request(line, paths, None, &Registry::default(), &Cancellation::default())
    }
}

// One application row, the shape ui/js/Menu.js and ui/OpenWithDialog.qml both read.
fn application_entries(apps: &[menu_registry::Application]) -> String {
    apps.iter().map(|a| format!(r#"{{"id":"{}","label":"{}","icon":"{}","default":{}}}"#,
        escape(&a.id), escape(&a.label), escape(&a.icon), a.default))
        .collect::<Vec<String>>().join(",")
}

pub(crate) fn response(line: &str, result: Result<String, String>) -> String {
    let header = format!(r#""t":"menuaction","id":{},"op":"{}""#,
        field_usize(line, "id").unwrap_or(0), escape(&field_str(line, "op").unwrap_or_default()));
    match result {
        Ok(fields) => format!("{{{},\"ok\":true{}{}}}", header, if fields.is_empty() { "" } else { "," }, fields),
        Err(error) => format!(r#"{{{},"ok":false,"error":"{}"}}"#, header, escape(&error)),
    }
}

// create_new performs collision refusal atomically, including a dangling symlink at the chosen name.
pub fn create_file(parent: &Path, name: &str) -> Result<(PathBuf, super::undo::ItemIdentity), String> {
    if !parent.is_absolute() || !super::ops::valid_name(name) {
        return Err("New File requires an absolute folder and one non-empty filename.".into());
    }
    let path = parent.join(name);
    let file = OpenOptions::new().write(true).create_new(true).open(&path)
        .map_err(|e| format!("Could not create {}: {}.", path.display(), crate::error::io_message(&e)))?;
    let meta = file.metadata().map_err(|e| format!("Created {}, but could not record its identity: {}.", path.display(), crate::error::io_message(&e)))?;
    Ok((path, super::undo::ItemIdentity::record(&meta)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::symlink;

    #[test]
    fn cancellation_at_publication_cannot_resurrect_a_closed_snapshot() {
        let published = Mutex::new(Snapshot::default());
        let old = Cancellation::default();
        let previously_checked = old.check().is_ok();
        old.next();
        if previously_checked { *published.lock().unwrap() = Snapshot { id: 3, ..Snapshot::default() }; }
        assert_eq!(published.lock().unwrap().id, 3, "the former check-before-lock ordering resurrects the snapshot");

        *published.lock().unwrap() = Snapshot::default();
        let current = Cancellation::default();
        let guard = published.lock().unwrap();
        current.next();
        publish_snapshot(guard, Snapshot { id: 3, ..Snapshot::default() }, &current);
        assert_eq!(published.lock().unwrap().id, 0, "publication must recheck the cancelled generation under its lock");
    }

    #[test]
    fn deletion_confirmation_is_replaced_consumed_and_closed() {
        let d = TestDir::new("menu-delete-token");
        let path = d.file("item", "preserved");
        let item = Selected::inspect(path.to_str().unwrap()).unwrap();
        let prepare = || Arc::new(super::super::menudelete::Review::prepare(std::slice::from_ref(&item), d.path(), &Cancellation::default()).unwrap());
        let old = prepare();
        let current = prepare();
        assert_ne!(old.token, current.token);
        let mut snapshot = Snapshot { id: 4, items: vec![item.clone()], deletion: Some(current.clone()), ..Snapshot::default() };
        assert!(snapshot.take_deletion(old.token).is_err());
        assert!(snapshot.deletion.is_none(), "a refused confirmation cannot be retried as a different token");
        snapshot.deletion = Some(current.clone());
        assert!(snapshot.take_deletion(current.token).is_ok());
        assert!(snapshot.take_deletion(current.token).is_err());
        snapshot.deletion = Some(prepare());
        snapshot.handle(r#"{"op":"close","id":4}"#, vec![]);
        assert!(snapshot.deletion.is_none());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "preserved");
    }

    #[test]
    fn deletion_refresh_requires_a_new_token_for_replacements_and_descendants() {
        let d = TestDir::new("menu-delete-refresh");
        let path = d.file("item", "original");
        let folder = d.dir("folder");
        let mut snapshot = Snapshot::default();
        let paths = vec![path.to_string_lossy().into(), folder.to_string_lossy().into()];
        snapshot.handle(r#"{"op":"snapshot","id":5}"#, paths);
        let first = snapshot.handle(r#"{"op":"prepareDelete","id":5}"#, vec![]);
        let old_token = field_usize(&first, "token").unwrap();
        d.file("folder/new", "new child");
        let check = format!(r#"{{"op":"checkDelete","id":5,"token":{}}}"#, old_token);
        assert!(snapshot.handle(&check, vec![]).contains(r#""valid":false"#));
        assert!(path.is_absolute() && path.starts_with(d.path()) && d.path().join(".flea-test-sandbox").is_file());
        std::fs::rename(&path, d.join("original-moved")).unwrap();
        d.file("item", "replacement");
        d.file("unrelated", "preserved");
        let fresh = snapshot.handle(r#"{"op":"refreshDelete","id":5}"#, vec![]);
        assert!(fresh.contains(r#""ok":true"#), "{fresh}");
        assert_eq!(field_usize(&fresh, "count"), Some(2));
        assert_ne!(field_usize(&fresh, "token"), Some(old_token));
        assert_eq!(snapshot.items.len(), 2, "unrelated arrivals never widen the confirmation");
        assert!(snapshot.items.iter().all(|item| item.current().is_ok()));
        assert!(snapshot.take_deletion(old_token).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
    }

    #[test]
    fn completed_deletion_reselection_rejects_replacements_after_close() {
        let d = TestDir::new("menu-delete-reselection");
        let path = d.file("item", "original");
        let kept = d.file("kept", "survivor");
        let (tx, _rx) = std::sync::mpsc::channel();
        let menu = MenuActions::new(tx);
        *menu.restoration.lock().unwrap() = (8, vec![Selected::inspect(path.to_str().unwrap()).unwrap(), Selected::inspect(kept.to_str().unwrap()).unwrap()]);
        assert!(path.is_absolute() && path.starts_with(d.path()) && d.path().join(".flea-test-sandbox").is_file());
        std::fs::rename(&path, d.join("moved")).unwrap();
        d.file("item", "replacement");
        menu.request(r#"{"op":"close","id":8}"#.into(), vec![], None);
        let mut matches = vec![(path.to_str().unwrap(), 0), (kept.to_str().unwrap(), 1)];
        menu.retain_survivors(8, &mut matches).unwrap();
        assert_eq!(matches, vec![(kept.to_str().unwrap(), 1)]);
        assert!(menu.retain_survivors(9, &mut matches).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    }

    #[test]
    fn snapshot_rejects_replaced_items_and_stale_request_ids() {
        let sandbox = TestDir::new("menu-snapshot");
        let path = sandbox.file("item", "original");
        let mut snapshot = Snapshot::default();
        assert!(snapshot.handle(r#"{"op":"snapshot","id":3}"#, vec![path.to_string_lossy().into()]).contains(r#""ok":true"#));
        assert!(snapshot.handle(r#"{"op":"activate","id":3,"action":"addFavourite"}"#, vec![]).contains(&format!(r#""paths":["{}"]"#, path.display())));
        assert!(snapshot.handle(r#"{"op":"validate","id":2}"#, vec![]).contains("expired"));
        let moved = sandbox.join("old");
        assert!(path.is_absolute() && path.starts_with(sandbox.path()) && moved.starts_with(sandbox.path()));
        std::fs::rename(&path, moved).unwrap();
        sandbox.file("item", "replacement");
        assert!(snapshot.handle(r#"{"op":"validate","id":3}"#, vec![]).contains("Selected item changed"));
        assert!(snapshot.handle(r#"{"op":"activate","id":3,"action":"addFavourite"}"#, vec![]).contains("Selected item changed"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    }
    #[test]
    fn create_file_refuses_collisions_traversal_and_symlinks() {
        let sandbox = TestDir::new("menu-newfile");
        let (path, identity) = create_file(sandbox.path(), "New File").unwrap();
        let meta = path.symlink_metadata().unwrap();
        assert_eq!(meta.len(), 0);
        assert_eq!(super::super::undo::ItemIdentity::record(&meta), identity);
        assert!(create_file(sandbox.path(), "New File").is_err());
        assert!(create_file(sandbox.path(), "../escape").is_err());
        assert!(create_file(sandbox.path(), "").is_err());
        assert!(create_file(Path::new("relative"), "file").is_err());
        symlink(sandbox.join("absent"), sandbox.join("link")).unwrap();
        assert!(create_file(sandbox.path(), "link").is_err());
        assert!(!sandbox.join("absent").exists());
    }
    #[test]
    fn cursor_provider_actions_validate_the_cursor_outside_the_marked_selection() {
        let sandbox = TestDir::new("menu-provider-cursor");
        let destination = sandbox.dir("Dropbox");
        let marked = sandbox.file("Dropbox/marked", "marked");
        let cursor = sandbox.file("Dropbox/cursor", "original");
        let mut snapshot = Snapshot::default();
        let reply = snapshot.handle_request(r#"{"op":"snapshot","id":11}"#, vec![marked.to_string_lossy().into()],
            cursor.to_str(), &Registry::default(), &Cancellation::default());
        assert!(crate::json::field_bool(&reply, "ok"));
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].path, marked);
        let capture = format!(r#"{{"op":"providerDestination","id":11,"dest":"{}"}}"#, escape(destination.to_str().unwrap()));
        assert!(crate::json::field_bool(&snapshot.handle(&capture, vec![]), "ok"));
        for action in ["taildrop:peer", "sharelink"] {
            let activate = format!(r#"{{"op":"activate","id":11,"action":"{}","dest":"{}"}}"#, action, escape(destination.to_str().unwrap()));
            let reply = snapshot.handle(&activate, vec![]);
            assert!(crate::json::field_bool(&reply, "ok"), "{}", reply);
            assert_eq!(crate::json::field_str_array(&reply, "paths"), vec![cursor.to_string_lossy().into_owned()]);
        }
        sandbox.assert_contains(&cursor);
        std::fs::rename(&cursor, sandbox.join("original-cursor")).unwrap();
        sandbox.file("Dropbox/cursor", "replacement");
        for action in ["taildrop:peer", "sharelink"] {
            let activate = format!(r#"{{"op":"activate","id":11,"action":"{}","dest":"{}"}}"#, action, escape(destination.to_str().unwrap()));
            let reply = snapshot.handle(&activate, vec![]);
            assert!(!crate::json::field_bool(&reply, "ok"));
            assert!(reply.contains("Selected item changed"));
            assert!(crate::json::field_str_array(&reply, "paths").is_empty());
        }
        snapshot.handle(r#"{"op":"snapshot","id":12}"#, vec![marked.to_string_lossy().into()]);
        assert!(snapshot.handle(r#"{"op":"activate","id":12,"action":"taildrop:peer"}"#, vec![]).contains("Cursor source was not captured"));
        assert_eq!(std::fs::read_to_string(marked).unwrap(), "marked");
        assert_eq!(std::fs::read_to_string(cursor).unwrap(), "replacement");
    }
    #[test]
    fn provider_activation_refuses_a_replaced_account_directory() {
        let sandbox = TestDir::new("menu-provider-destination");
        let source = sandbox.file("source", "keep");
        let destination = sandbox.dir("Dropbox");
        let mut snapshot = Snapshot::default();
        snapshot.handle(r#"{"op":"snapshot","id":10}"#, vec![source.to_string_lossy().into()]);
        let capture = format!(r#"{{"op":"providerDestination","id":10,"dest":"{}"}}"#, escape(destination.to_str().unwrap()));
        assert!(crate::json::field_bool(&snapshot.handle(&capture, vec![]), "ok"));
        let activate = format!(r#"{{"op":"activate","id":10,"action":"dropbox","dest":"{}"}}"#, escape(destination.to_str().unwrap()));
        assert!(crate::json::field_bool(&snapshot.handle(&activate, vec![]), "ok"));
        assert!(snapshot.provider_transfer);
        sandbox.assert_contains(&destination);
        std::fs::rename(&destination, sandbox.join("original-dropbox")).unwrap();
        sandbox.dir("Dropbox");
        assert!(snapshot.handle(&activate, vec![]).contains("Dropbox account folder changed"));
        assert_eq!(std::fs::read_to_string(source).unwrap(), "keep");
        assert!(!destination.join("source").exists());
    }
    #[test]
    fn properties_describe_the_link_itself_and_close_expires_selection() {
        let sandbox = TestDir::new("menu-link");
        let target = sandbox.file("target", "body");
        let link = sandbox.join("link");
        symlink(&target, &link).unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.handle(r#"{"op":"snapshot","id":9}"#, vec![link.to_string_lossy().into()]);
        let line = snapshot.handle(r#"{"op":"properties","id":9}"#, vec![]);
        assert!(line.contains(r#""symlink":true"#));
        assert_eq!(field_str(&line, "target"), Some(target.to_string_lossy().into()));
        snapshot.handle(r#"{"op":"close","id":9}"#, vec![]);
        assert!(snapshot.handle(r#"{"op":"properties","id":9}"#, vec![]).contains("expired"));
    }
}
