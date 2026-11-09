// Claim the exact reviewed payload and metadata before deletion; later arrivals keep their original names.
mod recovery;
mod cancel;
mod parents;
use parents::parent_at;
use cancel::remove_tree_cancellable;
use flea::bytename::ExactText as _;
#[cfg(test)]
mod cancel_tests;
pub use recovery::{recover, recovery_root, RecoveryReport};
use recovery::Recovery;

use crate::backend::trashmanifest::{Cancellation, Manifest, Op, Outcome, Records, Refusal, Refuse};
use crate::json::{escape, field_str, field_usize};
use crate::oflags::O_NOFOLLOW;
use std::ffi::{CString, OsStr, OsString};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(target_arch = "x86_64")]
const O_DIRECTORY: i32 = 0o200000;
#[cfg(target_arch = "aarch64")]
const O_DIRECTORY: i32 = 0o40000;
const RENAME_NOREPLACE: u32 = 1;
// A kernel without renameat2 flags answers EINVAL, an old FUSE daemon EOPNOTSUPP.
const EINVAL: i32 = 22;
const EOPNOTSUPP: i32 = 95;
static NEXT: AtomicUsize = AtomicUsize::new(1);
#[cfg(test)]
thread_local! {
    static MATCH_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
fn match_walks() -> usize {
    MATCH_WALKS.with(|walks| walks.get())
}
extern "C" {
    fn renameat2(
        oldfd: i32,
        old: *const std::os::raw::c_char,
        newfd: i32,
        new: *const std::os::raw::c_char,
        flags: u32,
    ) -> i32;
}

#[derive(Clone, Debug, PartialEq)]
struct Identity {
    dev: u64,
    ino: u64,
    size: u64,
    mtime: i64,
    nanos: i64,
    mode: u32,
}
impl Identity {
    fn saved(&self) -> String {
        format!("{},{},{},{},{},{}", self.dev, self.ino, self.size, self.mtime, self.nanos, self.mode)
    }
    // Sample identity: "2049,123,8,1788861600,0,33188".
    fn from_saved(text: &str) -> Result<Self, Refusal> {
        let parts: Vec<_> = text.split(',').collect();
        if parts.len() != 6 { return Err(Refusal::changed("Invalid Trash review identity.")); }
        Ok(Self {
            dev: parts[0].parse().ok().changed("Invalid Trash device.")?,
            ino: parts[1].parse().ok().changed("Invalid Trash inode.")?,
            size: parts[2].parse().ok().changed("Invalid Trash size.")?,
            mtime: parts[3].parse().ok().changed("Invalid Trash mtime.")?,
            nanos: parts[4].parse().ok().changed("Invalid Trash nanoseconds.")?,
            mode: parts[5].parse().ok().changed("Invalid Trash mode.")?,
        })
    }
    fn of(meta: &Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
            size: meta.len(),
            mtime: meta.mtime(),
            nanos: meta.mtime_nsec(),
            mode: meta.mode(),
        }
    }
    fn matches_survivor(&self, current: &Self) -> bool {
        if self.mode & 0o170000 == 0o040000 {
            // Removing reviewed children changes directory size and mtime, but never its inode or mode.
            self.dev == current.dev && self.ino == current.ino && self.mode == current.mode
        } else { self == current }
    }
}
struct Node {
    relative: PathBuf,
    identity: Identity,
    children: u64,
}
impl Node {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [self.identity.dev, self.identity.ino, self.identity.size,
            self.identity.mtime as u64, self.identity.nanos as u64, self.identity.mode as u64, self.children] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(self.relative.as_os_str().as_bytes());
        bytes
    }
    // Sample record: seven little-endian u64 fields followed by the raw relative filename bytes.
    fn decode(bytes: &[u8]) -> Result<Self, Refusal> {
        const HEADER: usize = 7 * 8;
        if bytes.len() < HEADER { return Err(Refusal::changed("Truncated Trash node review.")); }
        let number = |index: usize| u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap());
        let relative = PathBuf::from(OsString::from_vec(bytes[HEADER..].to_vec()));
        if relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
            return Err(Refusal::changed("Invalid relative Trash review path."));
        }
        Ok(Self {
            relative,
            identity: Identity { dev: number(0), ino: number(1), size: number(2),
                mtime: number(3) as i64, nanos: number(4) as i64, mode: number(5) as u32 },
            children: number(6),
        })
    }
    fn directory(&self) -> bool { self.identity.mode & 0o170000 == 0o040000 }
    fn deletion_identity_matches(&self, current: &Identity) -> bool {
        self.identity.matches_survivor(current)
    }
    fn claim_remove(&self, parent: &File, name: &OsStr, quarantine: &File, record_offset: u64) -> Result<(), Refusal> {
        if !self.deletion_identity_matches(&identity(&fd_path(parent, name)).changed("Trash child unreadable")?) {
            return Err(Refusal::new(Outcome::ChildChanged, "Trash child changed; remaining data was preserved."));
        }
        let claimed = format!("entry-{}", record_offset);
        let claimed = OsStr::new(&claimed);
        rename(parent, name, quarantine, claimed).map_err(|reason| Refusal::new(Outcome::ChildLeft, "Trash child could not be claimed").because(reason))?;
        let path = fd_path(quarantine, claimed);
        let removal = (|| {
            if !self.deletion_identity_matches(&identity(&path).changed("Claimed Trash child unreadable")?) {
                return Err(Refusal::new(Outcome::ChildChanged, "Trash child changed while being claimed."));
            }
            if self.directory() {
                // remove_dir refuses arrivals absent from the snapshot instead of deleting them recursively.
                std::fs::remove_dir(&path).refuse(Outcome::ChildLeft, "Trash directory has surviving items")
            } else { std::fs::remove_file(&path).refuse(Outcome::ChildLeft, "Trash child could not be removed") }
        })();
        if let Err(error) = removal {
            // A child that cannot go back stays in the quarantine, where remove_empty finds it; the reason it could not is logged.
            if let Err(reason) = rename(quarantine, claimed, parent, name) {
                Refusal::new(Outcome::ChildLeft, format!("Trash child could not be put back: {reason}")).logged();
            }
            return Err(error);
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Reviewed {
    path: PathBuf,
    info: PathBuf,
    payload: Identity,
    metadata: Identity,
    tree: Option<Records>,
    nodes: usize,
    bytes: u64,
}

fn matches(tree: &Records, base: &File, root: &OsStr, cancel: &Cancellation) -> Result<bool, Refusal> {
    #[cfg(test)]
    MATCH_WALKS.with(|walks| walks.set(walks.get() + 1));
    let mut offset = tree.start();
    while let Some(bytes) = tree.next(&mut offset).changed("Trash review unreadable")? {
        cancel.check().map_err(Refusal::cancelled)?;
        let node = Node::decode(&bytes)?;
        let Ok((parent, name)) = parent_at(base, root, &node.relative) else { return Ok(false); };
        let path = fd_path(&parent, &name);
        if identity(&path).ok().as_ref() != Some(&node.identity) { return Ok(false); }
        if node.directory() {
            let directory = open_dir(&path)?;
            let mut children = 0;
            for entry in std::fs::read_dir(fd_path(&directory, OsStr::new("."))).changed("Could not read a Trash folder")? {
                cancel.check().map_err(Refusal::cancelled)?;
                entry.changed("Could not read a Trash folder entry")?;
                children += 1;
            }
            if children != node.children || identity(&path).ok().as_ref() != Some(&node.identity) { return Ok(false); }
        }
    }
    Ok(true)
}

fn open_dir(path: &Path) -> Result<File, Refusal> {
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_DIRECTORY)
        .open(path)
        .map_err(|e| Refusal::os(Outcome::Changed, &format!("Could not open Trash folder {}", path.display()), &e))
}
fn fd_path(file: &File, name: &OsStr) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd())).join(name)
}
// The error is io_message's plain clause, so each caller can say it under the outcome of its own step.
fn rename(from: &File, old: &OsStr, to: &File, new: &OsStr) -> Result<(), String> {
    rename_claim(from, old, to, new).map_err(|(_, reason)| reason)
}
fn rename_claim(from: &File, old: &OsStr, to: &File, new: &OsStr) -> Result<(), (ErrorKind, String)> {
    let old = CString::new(old.as_bytes()).map_err(|_| (ErrorKind::InvalidInput, "Invalid Trash filename.".into()))?;
    let new = CString::new(new.as_bytes()).map_err(|_| (ErrorKind::InvalidInput, "Invalid quarantine filename.".into()))?;
    if unsafe {
        renameat2(
            from.as_raw_fd(),
            old.as_ptr(),
            to.as_raw_fd(),
            new.as_ptr(),
            RENAME_NOREPLACE,
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        // A kernel without the flag answers EINVAL or EOPNOTSUPP, so the path fallback moves it.
        return match error.raw_os_error() {
            Some(EINVAL) | Some(EOPNOTSUPP) => rename_by_path(from, &old, to, &new).map_err(|e| (e.kind(), crate::error::io_message(&e))),
            _ => Err((error.kind(), crate::error::io_message(&error))),
        };
    }
    Ok(())
}

// The fd rename above names no path, so the fallback resolves both ends through this process's own descriptor table.
fn rename_by_path(from: &File, old: &CString, to: &File, new: &CString) -> std::io::Result<()> {
    let from_path = PathBuf::from(format!("/proc/self/fd/{}", from.as_raw_fd())).join(OsStr::from_bytes(old.as_bytes()));
    let to_path = PathBuf::from(format!("/proc/self/fd/{}", to.as_raw_fd())).join(OsStr::from_bytes(new.as_bytes()));
    crate::backend::renamecompat::rename_noreplace(&from_path, &to_path)
}
fn identity(path: &Path) -> std::io::Result<Identity> {
    path.symlink_metadata().map(|m| Identity::of(&m))
}
// A review that cannot be stored refuses the deletion before it starts, whatever the disk said.
fn stored<T>(result: Result<T, String>, what: &str) -> Result<T, Refusal> {
    result.refuse(Outcome::Prepare(Op::Delete), what).map_err(|refusal| refusal.because("the review could not be saved"))
}

fn snapshot_tree(path: &Path, payload: &Identity, manifest: &mut Manifest, scratch: &Path,
    cancel: &Cancellation) -> Result<(Records, u64, usize), Refusal> {
        let files = open_dir(path.parent().changed("Missing Trash parent.")?)?;
        let name = path.file_name().changed("Missing Trash filename.")?;
        let mut queue = stored(Manifest::new(scratch), "Could not start the Trash review")?;
        stored(queue.append(&Node { relative: PathBuf::new(), identity: payload.clone(), children: 0 }.encode()), "Could not write the Trash review")?;
        let start = manifest.len();
        let mut cursor = 0;
        let mut bytes = 0u64;
        let mut nodes = 0usize;
        while let Some(record) = stored(queue.records().next(&mut cursor), "Could not read the Trash review")? {
            cancel.check().map_err(Refusal::said)?;
            let mut node = Node::decode(&record)?;
            let (parent, entry_name) = parent_at(&files, name, &node.relative)?;
            let path = fd_path(&parent, &entry_name);
            if identity(&path).changed("Trash item unreadable while reviewing")? != node.identity {
                return Err(Refusal::changed("Trash changed while reviewing its contents."));
            }
            if node.directory() {
                let directory = open_dir(&path)?;
                if Identity::of(&directory.metadata().changed("Trash folder unreadable while reviewing")?) != node.identity {
                    return Err(Refusal::changed("Trash directory changed while reviewing it."));
                }
                for entry in std::fs::read_dir(fd_path(&directory, OsStr::new("."))).changed("Could not read a Trash folder")? {
                    cancel.check().map_err(Refusal::said)?;
                    let child_name = entry.changed("Could not read a Trash folder entry")?.file_name();
                    let child = Node { relative: node.relative.join(&child_name),
                        identity: identity(&fd_path(&directory, &child_name)).changed("Trash item unreadable while reviewing")?, children: 0 };
                    stored(queue.append(&child.encode()), "Could not write the Trash review")?;
                    node.children += 1;
                }
                if Identity::of(&directory.metadata().changed("Trash folder unreadable while reviewing")?) != node.identity {
                    return Err(Refusal::changed("Trash directory changed while reviewing it."));
                }
            } else {
                bytes = bytes.checked_add(node.identity.size).changed("Trash byte total exceeds the supported range.")?;
            }
            stored(manifest.append(&node.encode()), "Could not write the Trash review")?;
            nodes = nodes.checked_add(1).changed("Trash file total exceeds the supported range.")?;
        }
        let tree = stored(manifest.range(start, manifest.len()), "Could not read the Trash review")?;
        if identity(path).ok().as_ref() != Some(payload) || !matches(&tree, &files, name, cancel)? {
            return Err(Refusal::changed("Trash changed while reviewing its contents."));
        }
        Ok((tree, bytes, nodes))
}
fn remove_tree(tree: &Records, quarantine: &File, refused: Refusal) -> Result<(), Refusal> {
    remove_tree_cancellable(tree, quarantine, refused, &Cancellation::default(), &mut || {})
}


// Sample .trashinfo: "[Trash Info]\nPath=/original/photo%20one.jpg\nDeletionDate=2026-09-08T10:00:00\n".
// Every refusal here is a damaged record, since a metadata file that parses badly was never the user's change.
fn original_path(metadata: &File, trash_root: &Path) -> Result<PathBuf, Refusal> {
    // Linux absolute paths passed to the restore syscall cannot exceed PATH_MAX bytes.
    const PATH_MAX: u64 = 4096;
    const MAX_ENCODED_PATH: u64 = PATH_MAX * 3;
    const PATH_PREFIX_BYTES: u64 = 5;
    let mut reader = BufReader::new(metadata);
    let mut section = false;
    let mut original = None;
    loop {
        let mut line = String::new();
        let bytes = reader.by_ref().take(MAX_ENCODED_PATH + PATH_PREFIX_BYTES + 1).read_line(&mut line)
            .refuse(Outcome::Damaged, "Could not parse Trash metadata")?;
        if bytes == 0 { break; }
        if !line.ends_with('\n') && bytes as u64 == MAX_ENCODED_PATH + PATH_PREFIX_BYTES + 1 {
            return Err(Refusal::new(Outcome::Damaged, "Trash metadata contains a line longer than the supported filesystem path."));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.starts_with('[') { section = line == "[Trash Info]"; continue; }
        if !section { continue; }
        if let Some(value) = line.strip_prefix("Path=") {
            if original.is_some() { return Err(Refusal::new(Outcome::Damaged, "Trash metadata contains duplicate original paths.")); }
            let decoded = PathBuf::from(OsString::from_vec(crate::paths::percent_decode_bytes(value)));
            if decoded.as_os_str().is_empty() || decoded.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
                return Err(Refusal::new(Outcome::Damaged, "Trash metadata contains an invalid original path."));
            }
            let path = if decoded.is_absolute() { decoded } else {
                let volume = if trash_root.file_name().and_then(OsStr::to_str).map(|name| name.starts_with(".Trash-")).unwrap_or(false) {
                    trash_root.parent()
                } else if trash_root.parent().and_then(Path::file_name) == Some(OsStr::new(".Trash")) {
                    trash_root.parent().and_then(Path::parent)
                } else { None }.refuse(Outcome::Damaged, "Relative Trash original path has no supported volume root.")?;
                volume.join(decoded)
            };
            if path.file_name().is_none() { return Err(Refusal::new(Outcome::Damaged, "Trash original path names a filesystem root.")); }
            original = Some(path);
        }
    }
    original.refuse(Outcome::Damaged, "Trash metadata has no original path.")
}

// Completed steps distinguish a record still held from a removal whose durability could not be confirmed.
fn finish(journal: Recovery, op: Op, done: bool) -> Result<(), Refusal> {
    journal.complete().map_err(|refusal| {
        let effect = done.then_some(op);
        let outcome = match refusal.outcome() {
            Outcome::RecordUnconfirmed(_) => Outcome::RecordUnconfirmed(effect),
            _ => Outcome::RecordLeft(effect),
        };
        refusal.wrap(outcome)
    })
}
// After rollback the original refusal stands; only the record cleanup failure is logged here.
fn settle(journal: Recovery, op: Op) {
    if let Err(left) = finish(journal, op, false) { left.logged(); }
}

impl Reviewed {
    pub fn inspect(path: PathBuf, gio_identity: &str) -> Result<Self, Refusal> {
        if !path.is_absolute()
            || path
                .parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
                != Some("files")
        {
            return Err(Refusal::changed(
                "Trash provider did not supply a supported backing path.",
            ));
        }
        let name = path.file_name().changed("Trash filename is missing.")?;
        let root = path
            .parent()
            .and_then(Path::parent)
            .changed("Trash root is missing.")?;
        let info = root.join("info").join(crate::backend::hometrash::info_name(name));
        let payload = identity(&path).changed("Trash item unreadable")?;
        if gio_identity != format!("l{}:{}", payload.dev, payload.ino) {
            return Err(Refusal::changed("Trash identity changed while reading it."));
        }
        let info_meta = info.symlink_metadata().changed("Trash metadata unreadable")?;
        if !info_meta.is_file() {
            return Err(Refusal::changed("Trash metadata is not a regular file."));
        }
        Ok(Self {
            path,
            info,
            payload,
            metadata: Identity::of(&info_meta),
            tree: None,
            nodes: 0,
            bytes: 0,
        })
    }
    pub fn snapshot(&mut self, manifest: &mut Manifest, scratch: &Path, cancel: &Cancellation) -> Result<u64, Refusal> {
        self.tree = None;
        let (tree, bytes, nodes) = snapshot_tree(&self.path, &self.payload, manifest, scratch, cancel).map_err(Refusal::logged)?;
        if !self.unchanged() { return Err(Refusal::changed("Trash changed while reviewing its contents.").logged()); }
        self.tree = Some(tree);
        self.nodes = nodes;
        self.bytes = bytes;
        Ok(bytes)
    }
    pub fn saved(&self) -> Result<String, Refusal> {
        let tree = self.tree.as_ref().changed("Trash contents have not been reviewed.").map_err(Refusal::logged)?;
        let (path, info) = (self.path.exact_text(), self.info.exact_text());
        Ok(format!(r#"{{"path":"{}","info":"{}","payload":"{}","metadata":"{}","start":{},"end":{},"nodes":{},"bytes":{}}}"#,
            escape(&path), escape(&info), self.payload.saved(), self.metadata.saved(), tree.start(), tree.end(), self.nodes, self.bytes))
    }
    pub fn from_saved(text: &str, manifest: &Manifest) -> Result<Self, Refusal> {
        Self::read_saved(text, manifest).map_err(Refusal::logged)
    }
    fn read_saved(text: &str, manifest: &Manifest) -> Result<Self, Refusal> {
        Ok(Self {
            path: flea::bytename::exact(field_str(text, "path").changed("Missing Trash review path.")?),
            info: flea::bytename::exact(field_str(text, "info").changed("Missing Trash review metadata path.")?),
            payload: Identity::from_saved(&field_str(text, "payload").changed("Missing Trash review identity.")?)?,
            metadata: Identity::from_saved(&field_str(text, "metadata").changed("Missing Trash metadata identity.")?)?,
            nodes: field_usize(text, "nodes").changed("Missing Trash review file total.")?,
            bytes: field_usize(text, "bytes").changed("Missing Trash review byte total.")? as u64,
            tree: Some(manifest.range(field_usize(text, "start").changed("Missing Trash review start.")? as u64,
                field_usize(text, "end").changed("Missing Trash review end.")? as u64).changed("Invalid Trash review range")?),
        })
    }
    pub fn bytes(&self, deadline: std::time::Instant) -> crate::backend::dirsize::DirSize {
        if !self.unchanged() {
            return crate::backend::dirsize::DirSize {
                bytes: 0,
                partial: true,
            };
        }
        let mut result = match self.path.symlink_metadata() {
            Ok(meta) if meta.is_dir() => crate::backend::dirsize::walk_until(&self.path, deadline),
            Ok(meta) => crate::backend::dirsize::DirSize {
                bytes: meta.len(),
                partial: false,
            },
            Err(_) => crate::backend::dirsize::DirSize {
                bytes: 0,
                partial: true,
            },
        };
        result.partial |= !self.unchanged();
        result
    }
    pub fn unchanged(&self) -> bool {
        identity(&self.path).ok().as_ref() == Some(&self.payload)
            && identity(&self.info).ok().as_ref() == Some(&self.metadata)
    }
    pub fn contents_unchanged(&self, cancel: &Cancellation) -> Result<bool, Refusal> {
        if !self.unchanged() { return Ok(false); }
        let Some(tree) = &self.tree else { return Ok(true); };
        (|| {
            let files = open_dir(self.path.parent().changed("Missing Trash parent.")?)?;
            matches(tree, &files, self.path.file_name().changed("Missing Trash filename.")?, cancel)
        })().map_err(Refusal::logged)
    }
    pub fn selection_identity(&self) -> String {
        // This ephemeral value is compared within one backend session, never parsed or persisted.
        format!("{:?}|{:?}", self.payload, self.metadata)
    }
    pub fn thumbnail(&self) -> Option<(PathBuf, i64)> {
        (self.thumbnail_path().is_some() && self.unchanged())
            .then(|| (self.path.clone(), self.payload.mtime))
    }
    pub fn thumbnail_path(&self) -> Option<&Path> {
        (self.payload.mode & 0o170000 == 0o100000).then_some(&self.path)
    }
    pub fn provider_identity(&self) -> String {
        format!("l{}:{}", self.payload.dev, self.payload.ino)
    }
    pub fn same_item(&self, other: &Self) -> bool {
        self.path == other.path && self.info == other.info
            && self.payload.dev == other.payload.dev && self.payload.ino == other.payload.ino
            && self.payload.mode & 0o170000 == other.payload.mode & 0o170000
            && self.metadata.dev == other.metadata.dev && self.metadata.ino == other.metadata.ino
    }
    // Cleanup failures after payload removal still retire this item's thumbnail namespace.
    pub fn departed_uri(&self, result: &Result<(), Refusal>) -> Option<String> {
        let completed = match result {
            Ok(()) => true,
            Err(refusal) => matches!(refusal.outcome(),
                Outcome::InfoLeft(_, _) | Outcome::FolderLeft(_, _)
                | Outcome::RecordLeft(Some(_)) | Outcome::RecordUnconfirmed(Some(_))),
        };
        completed.then(|| super::thumbcache::uri_for(&self.path))
    }
    pub fn restore(&self, original: &Path, recovery_root: &Path) -> Result<(), Refusal> {
        self.restore_after(original, recovery_root, || {})
    }
    fn restore_after(&self, original: &Path, recovery_root: &Path, before_claim: impl FnOnce()) -> Result<(), Refusal> {
        self.with_claimed(Op::Restore, recovery_root, Some(original), before_claim, |quarantine| {
            let metadata = crate::backend::regfile::open_if_regular(&fd_path(quarantine, OsStr::new("metadata")), O_NOFOLLOW)
                .map_err(|e| Refusal::changed(format!("Could not read claimed Trash metadata: {}", e)))?;
            if Identity::of(&metadata.metadata().changed("Claimed Trash metadata unreadable")?) != self.metadata {
                return Err(Refusal::changed("Trash metadata changed after being claimed."));
            }
            let destination = original_path(&metadata, self.path.parent().and_then(Path::parent).changed("Missing Trash root.")?)?;
            if destination != original || Identity::of(&metadata.metadata().changed("Claimed Trash metadata unreadable")?) != self.metadata {
                return Err(Refusal::new(Outcome::OriginalChanged, "The Trash item's original location changed; refresh the selection."));
            }
            let parent = destination.parent().changed("Original parent is missing.")?;
            // A gone original folder is recreated first, as undo and gio restore do.
            let destination_parent = std::fs::create_dir_all(parent).and_then(|()| parent.canonicalize())
                .map_err(|e| Refusal::os(Outcome::Blocked, &format!("Could not open original location {}", destination.display()), &e))?;
            let destination_directory = open_dir(&destination_parent).map_err(|refusal| refusal.wrap(Outcome::Blocked))?;
            rename(quarantine, OsStr::new("payload"), &destination_directory,
                destination.file_name().changed("Original filename is missing.")?)
                .map_err(|reason| Refusal::new(Outcome::Blocked, format!("Could not restore {} without overwriting: {}", destination.display(), reason)).because(reason))
        })
    }
    fn with_claimed(&self, op: Op, recovery_root: &Path, destination: Option<&Path>, before_claim: impl FnOnce(), perform: impl FnOnce(&File) -> Result<(), Refusal>) -> Result<(), Refusal> {
        self.claimed(op, recovery_root, destination, before_claim, perform).map_err(Refusal::logged)
    }
    fn claimed(&self, op: Op, recovery_root: &Path, destination: Option<&Path>, before_claim: impl FnOnce(), perform: impl FnOnce(&File) -> Result<(), Refusal>) -> Result<(), Refusal> {
        let files = open_dir(self.path.parent().changed("Missing Trash parent.")?)?;
        let infos = open_dir(self.info.parent().changed("Missing Trash metadata parent.")?)?;
        let name = self
            .path
            .file_name()
            .changed("Invalid Trash filename.")?;
        let info_name = self
            .info
            .file_name()
            .changed("Invalid Trash metadata name.")?;
        if identity(&fd_path(&files, name)).changed("Trash item unreadable")? != self.payload
            || identity(&fd_path(&infos, info_name)).changed("Trash metadata unreadable")? != self.metadata
        {
            return Err(Refusal::new(Outcome::ConfirmAgain, "Trash item changed; confirm again."));
        }
        let quarantine_name = format!(
            ".flea-delete-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let quarantine_path = fd_path(&files, OsStr::new(&quarantine_name));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&quarantine_path)
            .map_err(|e| Refusal::os(Outcome::Prepare(op), "Could not create Trash quarantine", &e))?;
        let quarantine = open_dir(&quarantine_path).map_err(|refusal| refusal.wrap(Outcome::Prepare(op)))?;
        let recovery = self.path.parent().unwrap().join(&quarantine_name);
        let mut journal = match Recovery::begin(recovery_root, &self.path, &files, &self.payload,
            Some((&self.info, &self.metadata, &infos)), &recovery, &quarantine,
            if destination.is_some() { None } else { self.tree.as_ref() }, destination) {
            Ok(journal) => journal,
            Err(error) => { let _ = std::fs::remove_dir(&quarantine_path); return Err(error.wrap(Outcome::NoRecord(op))); }
        };
        before_claim();
        if let Err(error) = rename(&files, name, &quarantine, OsStr::new("payload")) {
            match journal.remove_empty(&quarantine_path, &quarantine, &files) {
                Ok(()) => settle(journal, op),
                Err(left) => { left.logged(); }
            }
            return Err(Refusal::new(Outcome::Prepare(op), format!("Could not claim Trash item: {}", error)).because(error));
        }
        let info_claim = rename_claim(&infos, info_name, &quarantine, OsStr::new("metadata"));
        let checked = identity(&fd_path(&quarantine, OsStr::new("payload"))).ok().as_ref() == Some(&self.payload)
            && info_claim.is_ok()
            && identity(&fd_path(&quarantine, OsStr::new("metadata"))).ok().as_ref() == Some(&self.metadata);
        if !checked {
            let refused = match &info_claim {
                Err((ErrorKind::NotFound, _)) | Ok(()) => Refusal::new(Outcome::ConfirmAgain, "Trash changed; no operation attempted. Confirm again."),
                Err((_, reason)) => Refusal::new(Outcome::Prepare(op), format!("Could not claim Trash metadata: {reason}")).because(reason.clone()),
            };
            let payload_back = rename(&quarantine, OsStr::new("payload"), &files, name);
            let info_back = if info_claim.is_ok() {
                rename(&quarantine, OsStr::new("metadata"), &infos, info_name)
            } else {
                Ok(())
            };
            if payload_back.is_err() || info_back.is_err() {
                return Err(refused.wrap(Outcome::Kept(recovery.clone())));
            }
            match journal.remove_empty(&quarantine_path, &quarantine, &files) {
                Ok(()) => settle(journal, op),
                Err(left) => { left.logged(); }
            }
            return Err(refused);
        }
        let removal = perform(&quarantine);
        if let Err(error) = removal {
            let payload_back = rename(&quarantine, OsStr::new("payload"), &files, name);
            let info_back = rename(&quarantine, OsStr::new("metadata"), &infos, info_name);
            if payload_back.is_err() || info_back.is_err() {
                return Err(error.wrap(Outcome::Stuck(op, recovery)));
            }
            if journal.remove_empty(&quarantine_path, &quarantine, &files).is_err() {
                return Err(error.wrap(Outcome::StuckInTrash(op, recovery)));
            }
            if error.outcome() == &Outcome::Cancelled {
                finish(journal, op, false).map_err(Refusal::logged)?;
                return Err(error);
            }
            settle(journal, op);
            // A refusal that is a whole sentence already says what was done: nothing was deleted, or nothing was restored.
            let whole = match (op, error.outcome()) {
                (_, Outcome::NothingDeleted) => true,
                (Op::Restore, Outcome::Blocked) => false,
                (Op::Restore, _) => true,
                _ => false,
            };
            return Err(if whole { error } else { error.wrap(Outcome::Partial(op)) });
        }
        std::fs::remove_file(fd_path(&quarantine, OsStr::new("metadata")))
            .map_err(|e| Refusal::os(Outcome::InfoLeft(op, recovery.clone()), "Trash metadata cleanup after completion", &e))?;
        journal.remove_empty(&quarantine_path, &quarantine, &files)
            .map_err(|e| e.wrap(Outcome::FolderLeft(op, Some(recovery.clone()))))?;
        finish(journal, op, true)
    }
}

// Ordinary-path deletion shares the complete tree review; only Trash requires paired .trashinfo claims.
pub struct PathReview {
    path: PathBuf,
    payload: Identity,
    tree: Records,
}
impl PathReview {
    pub fn prepare(path: PathBuf, manifest: &mut Manifest, scratch: &Path,
        cancel: &Cancellation) -> Result<(Self, u64), Refusal> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(Refusal::changed("Permanent deletion requires an absolute item path; filesystem roots are refused.").logged());
        }
        let payload = identity(&path).changed("Item unreadable").map_err(Refusal::logged)?;
        let (tree, bytes, _) = snapshot_tree(&path, &payload, manifest, scratch, cancel).map_err(Refusal::logged)?;
        Ok((Self { path, payload, tree }, bytes))
    }
    pub fn unchanged(&self, cancel: &Cancellation) -> Result<bool, Refusal> {
        if identity(&self.path).ok().as_ref() != Some(&self.payload) { return Ok(false); }
        (|| {
            let parent = open_dir(self.path.parent().changed("Missing deletion parent.")?)?;
            matches(&self.tree, &parent, self.path.file_name().changed("Missing deletion filename.")?, cancel)
        })().map_err(Refusal::logged)
    }
    pub fn delete(&self, recovery_root: &Path) -> Result<(), Refusal> {
        self.delete_after(recovery_root, || {})
    }
    fn delete_after(&self, recovery_root: &Path, before_claim: impl FnOnce()) -> Result<(), Refusal> {
        self.removed(recovery_root, before_claim).map_err(Refusal::logged)
    }
    fn removed(&self, recovery_root: &Path, before_claim: impl FnOnce()) -> Result<(), Refusal> {
        let op = Op::Delete;
        let parent_path = self.path.parent().changed("Missing deletion parent.")?;
        let parent = open_dir(parent_path)?;
        let name = self.path.file_name().changed("Missing deletion filename.")?;
        if identity(&fd_path(&parent, name)).changed("Item unreadable")? != self.payload {
            return Err(Refusal::new(Outcome::ItemChanged, "Item changed; confirm again."));
        }
        let quarantine_name = format!(".flea-delete-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let quarantine_path = fd_path(&parent, OsStr::new(&quarantine_name));
        std::fs::DirBuilder::new().mode(0o700).create(&quarantine_path)
            .map_err(|e| Refusal::os(Outcome::Prepare(op), "Could not create deletion quarantine", &e))?;
        let quarantine = open_dir(&quarantine_path).map_err(|refusal| refusal.wrap(Outcome::Prepare(op)))?;
        let recovery = parent_path.join(&quarantine_name);
        let mut journal = match Recovery::begin(recovery_root, &self.path, &parent, &self.payload,
            None, &recovery, &quarantine, Some(&self.tree), None) {
            Ok(journal) => journal,
            Err(error) => { let _ = std::fs::remove_dir(&quarantine_path); return Err(error.wrap(Outcome::NoRecord(op))); }
        };
        before_claim();
        if let Err(error) = rename(&parent, name, &quarantine, OsStr::new("payload")) {
            match journal.remove_empty(&quarantine_path, &quarantine, &parent) {
                Ok(()) => settle(journal, op),
                Err(left) => { left.logged(); }
            }
            return Err(Refusal::new(Outcome::Prepare(op), format!("Could not claim item for deletion: {}", error)).because(error));
        }
        let removal = remove_tree(&self.tree, &quarantine, Refusal::new(Outcome::ItemChanged, "Item changed after confirmation; no deletion attempted."));
        if let Err(error) = removal {
            if rename(&quarantine, OsStr::new("payload"), &parent, name).is_err() {
                return Err(error.wrap(Outcome::Stuck(op, recovery)));
            }
            if journal.remove_empty(&quarantine_path, &quarantine, &parent).is_err() {
                return Err(error.wrap(Outcome::StuckAtOriginal(recovery)));
            }
            settle(journal, op);
            return Err(if error.outcome() == &Outcome::ItemChanged { error } else { error.wrap(Outcome::PartialAtOriginal) });
        }
        journal.remove_empty(&quarantine_path, &quarantine, &parent).map_err(|e| e.wrap(Outcome::FolderLeft(op, Some(recovery.clone()))))?;
        finish(journal, op, true)
    }
}

#[cfg(test)]
#[path = "trashdelete/refusal_tests.rs"]
mod refusal_tests;

// Compatibility deletion hooks used by trashdelete::tests.
#[cfg(test)]
#[path = "trashdelete/deletetest.rs"]
mod deletetest;

#[cfg(test)]
#[path = "trashdelete/tests.rs"]
mod tests;
