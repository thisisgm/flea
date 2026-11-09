// Recorded, identity-checked recovery of interrupted claims; only inactive journals may be replayed.
use flea::bytename::ExactText as _;
use super::{fd_path, identity, open_dir, rename, Identity, Node};
use crate::backend::trashmanifest::{Manifest, Op, Outcome, Records, Refusal, Refuse};
use crate::json::{escape, field_str, field_usize};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
#[path = "syncs.rs"]
mod syncs;
pub use syncs::FolderSyncs;

const FINISHED: &[u8] = b"flea recovery complete 1";

pub fn recovery_root() -> Result<PathBuf, String> {
    let state = match crate::userfile::env_dir("XDG_STATE_HOME") {
        Some(path) => path,
        None => crate::userfile::home()?.join(".local/state"),
    };
    Ok(state.join("flea/recovery"))
}

// A record that cannot be kept stops the step before anything is claimed; the caller names the step.
fn unkept(detail: &str) -> Refusal {
    Refusal::new(Outcome::NoRecord(Op::Delete), detail).because("the recovery record could not be written")
}
fn written<T>(result: Result<T, String>, what: &str) -> Result<T, Refusal> {
    result.map_err(|e| unkept(&format!("{what}: {e}")))
}
// Replay holds a record of what a claim moved: a record whose own file will not clear is a leftover, not a loss.
fn left(detail: &str, reason: &str) -> Refusal {
    Refusal::new(Outcome::RecordLeft(None), detail).because(reason)
}

pub(super) struct Recovery {
    path: PathBuf,
    record: Manifest,
}
impl Recovery {
    pub(super) fn begin(root: &Path, source: &Path, parent: &File, payload: &Identity,
        paired: Option<(&Path, &Identity, &File)>, quarantine_path: &Path, quarantine: &File,
        tree: Option<&Records>, destination: Option<&Path>) -> Result<Self, Refusal> {
        if !root.is_absolute() { return Err(unkept("Recovery storage requires an absolute folder.").because("the recovery folder is not valid")); }
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root)
            .map_err(|e| Refusal::os(Outcome::NoRecord(Op::Delete), &format!("Could not create recovery folder {}", root.display()), &e))?;
        let directory = open_dir(root)?;
        let name = quarantine_path.file_name().ok_or_else(|| unkept("Missing quarantine name."))?.to_string_lossy();
        let path = root.join(format!("{}.review", name));
        let mut record = written(Manifest::create(&path), "Could not start the recovery record")?;
        let (info, metadata, info_parent) = match paired {
            Some((path, identity, parent)) => (path.exact_text().into_owned(), identity.saved(),
                Identity::of(&parent.metadata().refuse(Outcome::NoRecord(Op::Delete), "Trash metadata folder unreadable")?).saved()),
            None => Default::default(),
        };
        let text = format!(r#"{{"version":1,"source":"{}","parent":"{}","payload":"{}","info":"{}","metadata":"{}","infoParent":"{}","quarantine":"{}","quarantineIdentity":"{}","destination":"{}"}}"#,
            escape(&source.exact_text()),
            Identity::of(&parent.metadata().refuse(Outcome::NoRecord(Op::Delete), "Trash folder unreadable")?).saved(), payload.saved(),
            escape(&info), metadata, info_parent, escape(&quarantine_path.exact_text()),
            Identity::of(&quarantine.metadata().refuse(Outcome::NoRecord(Op::Delete), "Quarantine unreadable")?).saved(),
            escape(destination.map(|path| path.exact_text().into_owned()).unwrap_or_default().as_str()));
        written(record.append(text.as_bytes()), "Could not write the recovery record")?;
        if let Some(tree) = tree {
            let mut offset = tree.start();
            while let Some(bytes) = written(tree.next(&mut offset), "Could not read the Trash review")? {
                written(record.append(&bytes), "Could not write the recovery record")?;
            }
        }
        written(record.sync(), "Could not sync the recovery record")?;
        directory.sync_all().map_err(|e| Refusal::os(Outcome::NoRecord(Op::Delete), "Could not sync recovery folder", &e))?;
        Ok(Self { path, record })
    }
    pub(super) fn complete(self) -> Result<(), Refusal> {
        if identity(&self.path).refuse(Outcome::RecordLeft(None), "Recovery record unreadable")?
            != Identity::of(&self.record.file().metadata().refuse(Outcome::RecordLeft(None), "Recovery record unreadable")?) {
            return Err(left("Recovery record changed; it was preserved.", "the record changed"));
        }
        std::fs::remove_file(&self.path)
            .map_err(|e| Refusal::os(Outcome::RecordLeft(None), &format!("Could not finish recovery record {}", self.path.display()), &e))?;
        let folder = self.path.parent().refuse(Outcome::RecordUnconfirmed(None), "Missing recovery folder.")?;
        // An open restore batch syncs the folder once at its close; the record's removal is only cleanup.
        if syncs::held(folder) { return Ok(()); }
        open_dir(folder)
            .map_err(|refusal| refusal.wrap(Outcome::RecordUnconfirmed(None)))?.sync_all()
            .map_err(|e| Refusal::os(Outcome::RecordUnconfirmed(None), "Could not sync completed recovery record", &e))
    }
    pub(super) fn remove_empty(&mut self, path: &Path, quarantine: &File, parent: &File) -> Result<(), Refusal> {
        let cleanup = |what: &str, e: &std::io::Error| Refusal::os(Outcome::RecordLeft(None), what, e);
        if !same_node(&identity(path).refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?,
            &Identity::of(&quarantine.metadata().refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?)) {
            return Err(left("Recovery quarantine changed before cleanup; it was preserved.", "the temporary folder changed"));
        }
        let mut entries = std::fs::read_dir(fd_path(quarantine, OsStr::new("."))).refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?;
        if let Some(entry) = entries.next() {
            entry.refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?;
            return Err(Refusal::new(Outcome::Held(path.to_path_buf()), format!("Recovery data remains at {}; no cleanup was recorded.", path.display())).because("it still holds items"));
        }
        quarantine.sync_all().map_err(|e| cleanup("Could not sync empty recovery quarantine", &e))?;
        self.record.append(FINISHED).and_then(|_| self.record.sync()).map_err(|e| left(&e, "the recovery record could not be written"))?;
        std::fs::remove_dir(path).map_err(|e| cleanup(&format!("Could not remove empty recovery quarantine {}", path.display()), &e))?;
        if syncs::held_open(parent) { return Ok(()); }
        parent.sync_all().map_err(|e| cleanup("Could not sync recovery quarantine removal", &e))
    }
    // Where the quarantine opened, its items stay recoverable at that path; before that, nothing was moved.
    fn replay(&mut self) -> Result<(), Refusal> {
        let mut opened = None;
        self.replay_into(&mut opened).map_err(|error| {
            if matches!(error.outcome(), Outcome::RecordLeft(_)) { return error; }
            error.wrap(match opened { Some(at) => Outcome::Held(at), None => Outcome::NothingMoved })
        })
    }
    fn replay_into(&mut self, opened: &mut Option<PathBuf>) -> Result<(), Refusal> {
        let records = self.record.records();
        let mut tree_start = 0;
        let header = records
            .next(&mut tree_start)
            .changed("Recovery record unreadable")?
            .changed("Recovery record has no header.")?;
        let header = String::from_utf8(header)
            .ok()
            .changed("Recovery header is not valid text.")?;
        if field_usize(&header, "version") != Some(1) {
            return Err(Refusal::changed("Unsupported recovery record version."));
        }
        let path = |key: &str| -> Result<PathBuf, Refusal> {
            let value = flea::bytename::exact(
                field_str(&header, key).changed(&format!("Recovery record has no {}.", key))?,
            );
            if !value.is_absolute()
                || value.file_name().is_none()
                || value
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(Refusal::changed(format!(
                    "Recovery record has an invalid {} path.",
                    key
                )));
            }
            Ok(value)
        };
        let expected = |key: &str| Identity::from_saved(&field_str(&header, key).changed(&format!("Recovery record has no {} identity.", key))?);
        let source = path("source")?;
        let quarantine_path = path("quarantine")?;
        let quarantine_name = quarantine_path.file_name().changed("Missing quarantine name.")?.to_string_lossy();
        let expected_name = format!("{}.review", quarantine_name);
        if !quarantine_name.starts_with(".flea-delete-") || self.path.file_name().and_then(OsStr::to_str) != Some(expected_name.as_str())
            || quarantine_path.parent() != source.parent() {
            return Err(Refusal::changed("Recovery record does not name its own quarantine."));
        }
        let parent = open_dir(source.parent().changed("Missing recovery source parent.")?)?;
        if !same_node(&Identity::of(&parent.metadata().changed("Recovery source folder unreadable")?), &expected("parent")?) {
            return Err(Refusal::changed("Recovery source folder changed; no item was moved."));
        }
        let quarantine = match open_dir(&quarantine_path) {
            Ok(directory) => directory,
            Err(error) => {
                if matches!(quarantine_path.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound) {
                    let mut end = records.end();
                    if records.previous(&mut end).changed("Recovery record unreadable")?.as_deref() == Some(FINISHED) { return Ok(()); }
                    return Err(Refusal::changed("Recovery quarantine is missing without a completion record; the journal was preserved."));
                }
                return Err(error);
            }
        };
        *opened = Some(quarantine_path.clone());
        if !same_node(&Identity::of(&quarantine.metadata().changed("Recovery quarantine unreadable")?), &expected("quarantineIdentity")?) {
            return Err(Refusal::changed(format!("Recovery quarantine changed; data preserved at {}.", quarantine_path.display())));
        }
        let payload = expected("payload")?;
        let name = source.file_name().changed("Missing recovery source name.")?;
        let claimed_payload = fd_path(&quarantine, OsStr::new("payload"));
        if let Some(current) = optional_identity(&claimed_payload)? {
            if !payload.matches_survivor(&current) { return Err(Refusal::changed("Recovery payload identity changed.")); }
        } else if let Some(current) = optional_identity(&fd_path(&parent, name))? {
            if same_node(&current, &payload) && !payload.matches_survivor(&current) {
                return Err(Refusal::changed("Returned recovery payload changed; metadata was preserved."));
            }
        }
        let metadata_path = fd_path(&quarantine, OsStr::new("metadata"));
        if let Some(current) = optional_identity(&metadata_path)? {
            if current != expected("metadata")? { return Err(Refusal::changed("Recovery Trash metadata changed.")); }
        }
        let interrupted_node = |name: &OsStr| -> Result<Option<Node>, Refusal> {
            if name == "payload" || name == "metadata" { return Ok(None); }
            // Sample claim name: "entry-72", the byte offset of its framed node in the saved tree.
            let offset = name.to_str().and_then(|name| name.strip_prefix("entry-")).and_then(|value| value.parse::<u64>().ok())
                .filter(|offset| name == OsStr::new(&format!("entry-{}", offset)))
                .changed("Unrecognized recovery data; no item was moved.")?;
            let mut offset = tree_start.checked_add(offset).changed("Invalid recovery child offset.")?;
            let record = records.next(&mut offset).changed("Recovery record unreadable")?.changed("Recovery child has no review record.")?;
            Ok(Some(Node::decode(&record)?))
        };
        // Validate all claims before returning children or cleaning completed metadata.
        for entry in std::fs::read_dir(fd_path(&quarantine, OsStr::new("."))).changed("Recovery quarantine unreadable")? {
            let claimed_name = entry.changed("Recovery quarantine unreadable")?.file_name();
            if let Some(node) = interrupted_node(&claimed_name)? {
                if !node.identity.matches_survivor(&identity(&fd_path(&quarantine, &claimed_name)).changed("Claimed recovery child unreadable")?) {
                    return Err(Refusal::changed("Claimed recovery child changed; no item was moved."));
                }
            }
        }
        for entry in std::fs::read_dir(fd_path(&quarantine, OsStr::new("."))).changed("Recovery quarantine unreadable")? {
            let entry = entry.changed("Recovery quarantine unreadable")?;
            let claimed_name = entry.file_name();
            let Some(node) = interrupted_node(&claimed_name)? else { continue; };
            if !node.identity.matches_survivor(&identity(&fd_path(&quarantine, &claimed_name)).changed("Claimed recovery child unreadable")?) {
                return Err(Refusal::changed("Claimed recovery child changed; no item was moved."));
            }
            let (child_parent, child_name) = if node.relative.as_os_str().is_empty() || identity(&fd_path(&quarantine, OsStr::new("payload"))).is_ok() {
                reviewed_parent(&records, tree_start, &quarantine, OsStr::new("payload"), &node.relative, &payload)?
            } else {
                if !payload.matches_survivor(&identity(&fd_path(&parent, name)).changed("Recovery payload unreadable")?) { return Err(Refusal::changed("Recovery payload is unavailable.")); }
                reviewed_parent(&records, tree_start, &parent, name, &node.relative, &payload)?
            };
            rename(&quarantine, &claimed_name, &child_parent, &child_name)
                .map_err(|e| Refusal::changed(format!("Could not return interrupted child without overwriting: {}", e)))?;
        }
        let had_payload = match claimed_payload.symlink_metadata() {
            Ok(metadata) => {
                if !payload.matches_survivor(&Identity::of(&metadata)) { return Err(Refusal::changed("Recovery payload identity changed.")); }
                rename(&quarantine, OsStr::new("payload"), &parent, name)
                    .map_err(|e| Refusal::changed(format!("Could not return interrupted item without overwriting: {}", e)))?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(Refusal::changed(error.to_string())),
        };
        let metadata = match metadata_path.symlink_metadata() {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(Refusal::changed(format!(
                    "Could not inspect interrupted Trash metadata: {}",
                    error
                )))
            }
        };
        if let Some(metadata) = metadata {
            if Identity::of(&metadata) != expected("metadata")? {
                return Err(Refusal::changed("Recovery Trash metadata changed."));
            }
            if had_payload
                || identity(&fd_path(&parent, name))
                    .map(|current| payload.matches_survivor(&current))
                    .unwrap_or(false)
            {
                let info = path("info")?;
                let infos = open_dir(info.parent().changed("Missing Trash metadata parent.")?)?;
                if !same_node(
                    &Identity::of(
                        &infos
                            .metadata()
                            .changed("Trash metadata folder unreadable")?,
                    ),
                    &expected("infoParent")?,
                ) {
                    return Err(Refusal::changed(
                        "Recovery Trash metadata folder changed; metadata was preserved.",
                    ));
                }
                rename(
                    &quarantine,
                    OsStr::new("metadata"),
                    &infos,
                    info.file_name().changed("Missing Trash metadata name.")?,
                )
                .map_err(|e| {
                    Refusal::changed(format!(
                        "Could not return interrupted Trash metadata without overwriting: {}",
                        e
                    ))
                })?;
            } else {
                let destination = field_str(&header, "destination").unwrap_or_default();
                if !destination.is_empty()
                    && !identity(&flea::bytename::view(&destination))
                        .map(|current| same_node(&current, &payload))
                        .unwrap_or(false)
                {
                    return Err(Refusal::changed(
                        "Interrupted restore has no verified destination; metadata was preserved.",
                    ));
                }
                std::fs::remove_file(&metadata_path)
                    .changed("Could not clean completed Trash metadata")?;
            }
        }
        self.remove_empty(&quarantine_path, &quarantine, &parent)?;
        Ok(())
    }
}
fn same_node(left: &Identity, right: &Identity) -> bool {
    left.dev == right.dev && left.ino == right.ino && left.mode & 0o170000 == right.mode & 0o170000
}
fn optional_identity(path: &Path) -> Result<Option<Identity>, Refusal> {
    match path.symlink_metadata() {
        Ok(metadata) => Ok(Some(Identity::of(&metadata))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Refusal::changed(format!("Could not inspect recovery item {}: {}", path.display(), error))),
    }
}

fn reviewed_parent(records: &Records, tree_start: u64, base: &File, root: &OsStr,
    relative: &Path, payload: &Identity) -> Result<(File, OsString), Refusal> {
    if relative.as_os_str().is_empty() {
        return Ok((base.try_clone().changed("Could not duplicate a recovery handle")?, root.to_os_string()));
    }
    let mut directory = open_dir(&fd_path(base, root))?;
    if !payload.matches_survivor(&Identity::of(&directory.metadata().changed("Recovery payload unreadable")?)) {
        return Err(Refusal::changed("Recovery payload changed before returning its child."));
    }
    let mut cursor = tree_start;
    let mut prefix = PathBuf::new();
    for component in relative.parent().changed("Missing recovery child parent.")?.components() {
        let std::path::Component::Normal(name) = component else { return Err(Refusal::changed("Invalid recovery child parent.")); };
        prefix.push(name);
        // Saved nodes are breadth-first, so each deeper ancestor follows the previous one in the stream.
        let reviewed = loop {
            let record = records.next(&mut cursor).changed("Recovery record unreadable")?.changed("Recovery ancestor has no review record.")?;
            let node = Node::decode(&record)?;
            if node.relative == prefix { break node; }
        };
        let next = open_dir(&fd_path(&directory, name))?;
        if !reviewed.identity.matches_survivor(&Identity::of(&next.metadata().changed("Recovery child folder unreadable")?)) {
            return Err(Refusal::changed("Recovery child folder changed; no child was moved."));
        }
        directory = next;
    }
    Ok((directory, relative.file_name().changed("Missing recovery child name.")?.to_os_string()))
}

#[derive(Default)]
pub struct RecoveryReport {
    pub recovered: usize,
    pub failures: Vec<String>,
}
pub fn recover(root: &Path) -> Result<RecoveryReport, Refusal> {
    if !root.is_absolute() { return Err(Refusal::new(Outcome::NothingMoved, "Recovery storage requires an absolute folder.").logged()); }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(RecoveryReport::default()),
        Err(error) => return Err(Refusal::os(Outcome::NothingMoved, &format!("Could not read recovery folder {}", root.display()), &error).logged()),
    };
    recover_entries(root, entries)
}
fn recover_entries(root: &Path, entries: impl IntoIterator<Item = std::io::Result<std::fs::DirEntry>>) -> Result<RecoveryReport, Refusal> {
    let mut result = RecoveryReport::default();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                result.failures.push(format!("{}: {}", root.display(),
                    Refusal::os(Outcome::RecoveryUnreadable(root.to_path_buf()), "Could not read the recovery folder", &error).logged()));
                break;
            }
        };
        let name = entry.file_name();
        if !name.to_str().map(|name| name.starts_with(".flea-delete-") && name.ends_with(".review")).unwrap_or(false) { continue; }
        let path = entry.path();
        let result_for_record = (|| {
            let Some(record) = Manifest::open_inactive(&path).refuse(Outcome::NothingMoved, "Could not open the recovery record")? else { return Ok(false); };
            let mut recovery = Recovery { path: path.clone(), record };
            recovery.replay()?;
            recovery.complete()?;
            Ok::<bool, Refusal>(true)
        })();
        match result_for_record {
            Ok(true) => result.recovered += 1,
            Ok(false) => {}
            Err(error) => result.failures.push(format!("{}: {}", path.display(), error.logged())),
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
