// Recorded, identity-checked recovery of interrupted claims; only inactive journals may be replayed.
use super::{fd_path, identity, open_dir, rename, Identity, Node};
use crate::backend::trashmanifest::{Manifest, Records};
use crate::json::{escape, field_str, field_usize};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

const FINISHED: &[u8] = b"flea recovery complete 1";

pub fn recovery_root() -> Result<PathBuf, String> {
    let state = match crate::userfile::env_dir("XDG_STATE_HOME") {
        Some(path) => path,
        None => crate::userfile::home()?.join(".local/state"),
    };
    Ok(state.join("flea/recovery"))
}

pub(super) struct Recovery {
    path: PathBuf,
    record: Manifest,
}
impl Recovery {
    pub(super) fn begin(root: &Path, source: &Path, parent: &File, payload: &Identity,
        paired: Option<(&Path, &Identity, &File)>, quarantine_path: &Path, quarantine: &File,
        tree: Option<&Records>, destination: Option<&Path>) -> Result<Self, String> {
        if !root.is_absolute() { return Err("Recovery storage requires an absolute directory.".into()); }
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root)
            .map_err(|e| format!("Could not create recovery directory {}: {}", root.display(), e))?;
        let directory = open_dir(root)?;
        let name = quarantine_path.file_name().ok_or("Missing quarantine name.")?.to_string_lossy();
        let path = root.join(format!("{}.review", name));
        let mut record = Manifest::create(&path)?;
        let (info, metadata, info_parent) = match paired {
            Some((path, identity, parent)) => (path.to_string_lossy().into_owned(), identity.saved(),
                Identity::of(&parent.metadata().map_err(|e| e.to_string())?).saved()),
            None => Default::default(),
        };
        let text = format!(r#"{{"version":1,"source":"{}","parent":"{}","payload":"{}","info":"{}","metadata":"{}","infoParent":"{}","quarantine":"{}","quarantineIdentity":"{}","destination":"{}"}}"#,
            escape(source.to_str().ok_or("Recovery source path is not valid text.")?),
            Identity::of(&parent.metadata().map_err(|e| e.to_string())?).saved(), payload.saved(),
            escape(&info), metadata, info_parent, escape(quarantine_path.to_str().ok_or("Recovery path is not valid text.")?),
            Identity::of(&quarantine.metadata().map_err(|e| e.to_string())?).saved(),
            escape(destination.map(|path| path.to_string_lossy().into_owned()).unwrap_or_default().as_str()));
        record.append(text.as_bytes())?;
        if let Some(tree) = tree {
            let mut offset = tree.start();
            while let Some(bytes) = tree.next(&mut offset)? { record.append(&bytes)?; }
        }
        record.sync()?;
        directory.sync_all().map_err(|e| format!("Could not sync recovery directory: {}", e))?;
        Ok(Self { path, record })
    }
    pub(super) fn complete(self) -> Result<(), String> {
        if identity(&self.path)? != Identity::of(&self.record.file().metadata().map_err(|e| e.to_string())?) {
            return Err("Recovery record changed; it was preserved.".into());
        }
        std::fs::remove_file(&self.path)
            .map_err(|e| format!("Could not finish recovery record {}: {}", self.path.display(), e))?;
        open_dir(self.path.parent().ok_or("Missing recovery directory.")?)?.sync_all()
            .map_err(|e| format!("Could not sync completed recovery record: {}", e))
    }
    pub(super) fn remove_empty(&mut self, path: &Path, quarantine: &File, parent: &File) -> Result<(), String> {
        if !same_node(&identity(path)?, &Identity::of(&quarantine.metadata().map_err(|e| e.to_string())?)) {
            return Err("Recovery quarantine changed before cleanup; it was preserved.".into());
        }
        let mut entries = std::fs::read_dir(fd_path(quarantine, OsStr::new("."))).map_err(|e| e.to_string())?;
        if let Some(entry) = entries.next() {
            entry.map_err(|e| e.to_string())?;
            return Err(format!("Recovery data remains at {}; no cleanup was recorded.", path.display()));
        }
        quarantine.sync_all().map_err(|e| format!("Could not sync empty recovery quarantine: {}", e))?;
        self.record.append(FINISHED)?;
        self.record.sync()?;
        std::fs::remove_dir(path).map_err(|e| format!("Could not remove empty recovery quarantine {}: {}", path.display(), e))?;
        parent.sync_all().map_err(|e| format!("Could not sync recovery quarantine removal: {}", e))
    }
    fn replay(&mut self) -> Result<(), String> {
        let records = self.record.records();
        let mut tree_start = 0;
        let header = records.next(&mut tree_start)?.ok_or("Recovery record has no header.")?;
        let header = String::from_utf8(header).map_err(|_| "Recovery header is not valid text.")?;
        if field_usize(&header, "version") != Some(1) { return Err("Unsupported recovery record version.".into()); }
        let path = |key: &str| -> Result<PathBuf, String> {
            let value = PathBuf::from(field_str(&header, key).ok_or_else(|| format!("Recovery record has no {}.", key))?);
            if !value.is_absolute() || value.file_name().is_none() || value.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
                return Err(format!("Recovery record has an invalid {} path.", key));
            }
            Ok(value)
        };
        let expected = |key: &str| Identity::from_saved(&field_str(&header, key).ok_or_else(|| format!("Recovery record has no {} identity.", key))?);
        let source = path("source")?;
        let quarantine_path = path("quarantine")?;
        let quarantine_name = quarantine_path.file_name().ok_or("Missing quarantine name.")?.to_string_lossy();
        let expected_name = format!("{}.review", quarantine_name);
        if !quarantine_name.starts_with(".flea-delete-") || self.path.file_name().and_then(OsStr::to_str) != Some(expected_name.as_str())
            || quarantine_path.parent() != source.parent() {
            return Err("Recovery record does not name its own quarantine.".into());
        }
        let parent = open_dir(source.parent().ok_or("Missing recovery source parent.")?)?;
        if !same_node(&Identity::of(&parent.metadata().map_err(|e| e.to_string())?), &expected("parent")?) {
            return Err("Recovery source directory changed; no item was moved.".into());
        }
        let quarantine = match open_dir(&quarantine_path) {
            Ok(directory) => directory,
            Err(error) => {
                if matches!(quarantine_path.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound) {
                    let mut end = records.end();
                    if records.previous(&mut end)?.as_deref() == Some(FINISHED) { return Ok(()); }
                    return Err("Recovery quarantine is missing without a completion record; the journal was preserved.".into());
                }
                return Err(error);
            }
        };
        if !same_node(&Identity::of(&quarantine.metadata().map_err(|e| e.to_string())?), &expected("quarantineIdentity")?) {
            return Err(format!("Recovery quarantine changed; data preserved at {}.", quarantine_path.display()));
        }
        let payload = expected("payload")?;
        let name = source.file_name().ok_or("Missing recovery source name.")?;
        let claimed_payload = fd_path(&quarantine, OsStr::new("payload"));
        if let Some(current) = optional_identity(&claimed_payload)? {
            if !payload.matches_survivor(&current) { return Err("Recovery payload identity changed.".into()); }
        } else if let Some(current) = optional_identity(&fd_path(&parent, name))? {
            if same_node(&current, &payload) && !payload.matches_survivor(&current) {
                return Err("Returned recovery payload changed; metadata was preserved.".into());
            }
        }
        let metadata_path = fd_path(&quarantine, OsStr::new("metadata"));
        if let Some(current) = optional_identity(&metadata_path)? {
            if current != expected("metadata")? { return Err("Recovery Trash metadata changed.".into()); }
        }
        let interrupted_node = |name: &OsStr| -> Result<Option<Node>, String> {
            if name == "payload" || name == "metadata" { return Ok(None); }
            // Sample claim name: "entry-72", the byte offset of its framed node in the saved tree.
            let offset = name.to_str().and_then(|name| name.strip_prefix("entry-")).and_then(|value| value.parse::<u64>().ok())
                .filter(|offset| name == OsStr::new(&format!("entry-{}", offset)))
                .ok_or("Unrecognized recovery data; no item was moved.")?;
            let mut offset = tree_start.checked_add(offset).ok_or("Invalid recovery child offset.")?;
            Ok(Some(Node::decode(&records.next(&mut offset)?.ok_or("Recovery child has no review record.")?)?))
        };
        // Validate all claims before returning children or cleaning completed metadata.
        for entry in std::fs::read_dir(fd_path(&quarantine, OsStr::new("."))).map_err(|e| e.to_string())? {
            let claimed_name = entry.map_err(|e| e.to_string())?.file_name();
            if let Some(node) = interrupted_node(&claimed_name)? {
                if !node.identity.matches_survivor(&identity(&fd_path(&quarantine, &claimed_name))?) {
                    return Err("Claimed recovery child changed; no item was moved.".into());
                }
            }
        }
        for entry in std::fs::read_dir(fd_path(&quarantine, OsStr::new("."))).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let claimed_name = entry.file_name();
            let Some(node) = interrupted_node(&claimed_name)? else { continue; };
            if !node.identity.matches_survivor(&identity(&fd_path(&quarantine, &claimed_name))?) {
                return Err("Claimed recovery child changed; no item was moved.".into());
            }
            let (child_parent, child_name) = if node.relative.as_os_str().is_empty() || identity(&fd_path(&quarantine, OsStr::new("payload"))).is_ok() {
                reviewed_parent(&records, tree_start, &quarantine, OsStr::new("payload"), &node.relative, &payload)?
            } else {
                if !payload.matches_survivor(&identity(&fd_path(&parent, name))?) { return Err("Recovery payload is unavailable.".into()); }
                reviewed_parent(&records, tree_start, &parent, name, &node.relative, &payload)?
            };
            rename(&quarantine, &claimed_name, &child_parent, &child_name)
                .map_err(|e| format!("Could not return interrupted child without overwriting: {}", e))?;
        }
        let had_payload = match claimed_payload.symlink_metadata() {
            Ok(metadata) => {
                if !payload.matches_survivor(&Identity::of(&metadata)) { return Err("Recovery payload identity changed.".into()); }
                rename(&quarantine, OsStr::new("payload"), &parent, name)
                    .map_err(|e| format!("Could not return interrupted item without overwriting: {}", e))?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        let metadata = match metadata_path.symlink_metadata() {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Could not inspect interrupted Trash metadata: {}", error)),
        };
        if let Some(metadata) = metadata {
            if Identity::of(&metadata) != expected("metadata")? { return Err("Recovery Trash metadata changed.".into()); }
            if had_payload || identity(&fd_path(&parent, name)).map(|current| payload.matches_survivor(&current)).unwrap_or(false) {
                let info = path("info")?;
                let infos = open_dir(info.parent().ok_or("Missing Trash metadata parent.")?)?;
                if !same_node(&Identity::of(&infos.metadata().map_err(|e| e.to_string())?), &expected("infoParent")?) {
                    return Err("Recovery Trash metadata directory changed; metadata was preserved.".into());
                }
                rename(&quarantine, OsStr::new("metadata"), &infos, info.file_name().ok_or("Missing Trash metadata name.")?)
                    .map_err(|e| format!("Could not return interrupted Trash metadata without overwriting: {}", e))?;
            } else {
                let destination = field_str(&header, "destination").unwrap_or_default();
                if !destination.is_empty() && !identity(Path::new(&destination)).map(|current| same_node(&current, &payload)).unwrap_or(false) {
                    return Err("Interrupted restore has no verified destination; metadata was preserved.".into());
                }
                std::fs::remove_file(&metadata_path).map_err(|e| format!("Could not clean completed Trash metadata: {}", e))?;
            }
        }
        self.remove_empty(&quarantine_path, &quarantine, &parent)?;
        Ok(())
    }
}
fn same_node(left: &Identity, right: &Identity) -> bool {
    left.dev == right.dev && left.ino == right.ino && left.mode & 0o170000 == right.mode & 0o170000
}
fn optional_identity(path: &Path) -> Result<Option<Identity>, String> {
    match path.symlink_metadata() {
        Ok(metadata) => Ok(Some(Identity::of(&metadata))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not inspect recovery item {}: {}", path.display(), error)),
    }
}

fn reviewed_parent(records: &Records, tree_start: u64, base: &File, root: &OsStr,
    relative: &Path, payload: &Identity) -> Result<(File, OsString), String> {
    if relative.as_os_str().is_empty() {
        return Ok((base.try_clone().map_err(|e| e.to_string())?, root.to_os_string()));
    }
    let mut directory = open_dir(&fd_path(base, root))?;
    if !payload.matches_survivor(&Identity::of(&directory.metadata().map_err(|e| e.to_string())?)) {
        return Err("Recovery payload changed before returning its child.".into());
    }
    let mut cursor = tree_start;
    let mut prefix = PathBuf::new();
    for component in relative.parent().ok_or("Missing recovery child parent.")?.components() {
        let std::path::Component::Normal(name) = component else { return Err("Invalid recovery child parent.".into()); };
        prefix.push(name);
        // Saved nodes are breadth-first, so each deeper ancestor follows the previous one in the stream.
        let reviewed = loop {
            let node = Node::decode(&records.next(&mut cursor)?.ok_or("Recovery ancestor has no review record.")?)?;
            if node.relative == prefix { break node; }
        };
        let next = open_dir(&fd_path(&directory, name))?;
        if !reviewed.identity.matches_survivor(&Identity::of(&next.metadata().map_err(|e| e.to_string())?)) {
            return Err("Recovery child directory changed; no child was moved.".into());
        }
        directory = next;
    }
    Ok((directory, relative.file_name().ok_or("Missing recovery child name.")?.to_os_string()))
}

#[derive(Default)]
pub struct RecoveryReport {
    pub recovered: usize,
    pub failures: Vec<String>,
}
pub fn recover(root: &Path) -> Result<RecoveryReport, String> {
    if !root.is_absolute() { return Err("Recovery storage requires an absolute directory.".into()); }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(RecoveryReport::default()),
        Err(error) => return Err(format!("Could not read recovery directory {}: {}", root.display(), error)),
    };
    let mut result = RecoveryReport::default();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if !name.to_str().map(|name| name.starts_with(".flea-delete-") && name.ends_with(".review")).unwrap_or(false) { continue; }
        let path = entry.path();
        let result_for_record = (|| {
            let Some(record) = Manifest::open_inactive(&path)? else { return Ok(false); };
            let mut recovery = Recovery { path: path.clone(), record };
            recovery.replay()?;
            recovery.complete()?;
            Ok::<bool, String>(true)
        })();
        match result_for_record {
            Ok(true) => result.recovered += 1,
            Ok(false) => {}
            Err(error) => result.failures.push(format!("{}: {}", path.display(), error)),
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
