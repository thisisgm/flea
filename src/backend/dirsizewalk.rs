use super::{DevLookup, Progress, Tally};
use crate::backend::copyfile::{held_path, open_dir};
use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;

type Identity = (u64, u64);

struct Folder {
    name: OsString,
    identity: Identity,
}

struct Walk<'a> {
    tally: Tally,
    stop: &'a dyn Fn() -> bool,
    start_dev: Option<u64>,
    dev_of: DevLookup<'a>,
    progress: Option<Progress<'a>>,
    reported: Instant,
}

impl Walk<'_> {
    fn count(&mut self, size: u64) {
        self.tally.bytes += size;
        self.tally.items += 1;
        if let Some(p) = self.progress.as_mut() {
            if self.reported.elapsed() >= p.every {
                (p.report)(&self.tally);
                self.reported = Instant::now();
            }
        }
    }
}

pub(crate) fn walk_core<'a>(path: &Path, stop: &'a dyn Fn() -> bool, dev_of: DevLookup<'a>, progress: Option<Progress<'a>>) -> Tally {
    let start = path.symlink_metadata();
    let start_dev = start.as_ref().map(|m| m.dev()).ok();
    let expected = start.as_ref().map(identity).ok();
    let tally = match start {
        Ok(meta) => Tally { bytes: meta.size(), items: 0, partial: false },
        Err(_) => Tally { bytes: 0, items: 0, partial: true },
    };
    let mut walk = Walk { tally, stop, start_dev, dev_of, progress, reported: Instant::now() };
    if let Some(dir) = expected.and_then(|expected| open_same(path, expected)) {
        if walk_into(dir, path, &mut walk).is_none() { walk.tally.partial = true; }
    } else { walk.tally.partial = true; }
    if path.symlink_metadata().ok().as_ref().map(identity) != expected { walk.tally.partial = true; }
    walk.tally
}

fn identity(meta: &Metadata) -> Identity { (meta.dev(), meta.ino()) }

fn open_same(path: &Path, expected: Identity) -> Option<File> {
    let dir = open_dir(path).ok()?;
    (identity(&dir.metadata().ok()?) == expected).then_some(dir)
}

pub(super) fn same_device(start: Option<u64>, child: u64) -> bool {
    match start {
        Some(device) => device == child,
        None => true,
    }
}

// Moving the held descriptor into the child and returning through its checked parent keeps depth from holding descriptors.
fn walk_into(mut dir: File, path: &Path, walk: &mut Walk<'_>) -> Option<File> {
    let parent_identity = identity(&dir.metadata().ok()?);
    let mut folders = Vec::new();
    list_into(&dir, path, walk, &mut folders);
    for folder in folders {
        if (walk.stop)() { walk.tally.partial = true; break; }
        let at = held_path(&dir).join(&folder.name);
        let Some(child) = open_same(&at, folder.identity) else { walk.tally.partial = true; continue; };
        drop(dir);
        let child = walk_into(child, &path.join(&folder.name), walk)?;
        let parent = open_same(&held_path(&child).join(".."), parent_identity);
        drop(child);
        let Some(parent) = parent else { walk.tally.partial = true; return None; };
        dir = parent;
        if held_path(&dir).join(&folder.name).symlink_metadata().ok().as_ref().map(identity) != Some(folder.identity) {
            walk.tally.partial = true;
        }
    }
    Some(dir)
}

fn list_into(dir: &File, path: &Path, walk: &mut Walk<'_>, folders: &mut Vec<Folder>) {
    if let Some(entering) = walk.progress.as_ref().and_then(|p| p.entering) { entering(path); }
    if (walk.stop)() { walk.tally.partial = true; return; }
    let entries = match std::fs::read_dir(held_path(dir)) {
        Ok(entries) => entries,
        Err(_) => { walk.tally.partial = true; return; }
    };
    for entry in entries {
        if (walk.stop)() { walk.tally.partial = true; return; }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => { walk.tally.partial = true; continue; }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => { walk.tally.partial = true; continue; }
        };
        let named = path.join(entry.file_name());
        let known_dev = if file_type.is_dir() { walk.dev_of.and_then(|lookup| lookup(&named)) } else { None };
        let crossing = known_dev.is_some_and(|device| !same_device(walk.start_dev, device));
        let entering = walk.progress.as_ref().and_then(|p| p.entering).filter(|_| crossing);
        if let Some(entering) = entering { entering(&named); }
        let stat = entry.metadata();
        if let Some(entering) = entering { entering(path); }
        let meta = match stat {
            Ok(meta) => meta,
            Err(_) => { walk.tally.partial = true; continue; }
        };
        walk.count(meta.size());
        if file_type != meta.file_type() { walk.tally.partial = true; continue; }
        let child_dev = known_dev.unwrap_or_else(|| meta.dev());
        if file_type.is_dir() && same_device(walk.start_dev, child_dev) {
            folders.push(Folder { name: entry.file_name(), identity: identity(&meta) });
        }
    }
}

#[cfg(test)]
#[path = "dirsize_open_tests.rs"]
mod open_tests;
