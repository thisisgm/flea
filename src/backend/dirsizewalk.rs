use super::stat::{self, open_same, Entry, Identity};
use super::{Arm, DevLookup, Progress, Tally};
use crate::backend::copyfile::held_path;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::path::Path;
use std::time::Instant;

// Linked files one walk accounts for, about 4 MiB of map at most; one past it and the tree is never kept. Tests take 4 to reach it.
pub(super) const LINKED_CAP: usize = if cfg!(test) { 4 } else { 65_536 };

struct Folder {
    name: OsString,
    identity: Identity,
}

struct Walk<'a> {
    tally: Tally,
    stop: &'a dyn Fn() -> bool,
    start_dev: Option<u64>,
    start_mount: Option<u64>,
    dev_of: DevLookup<'a>,
    progress: Option<Progress<'a>>,
    arm: Arm<'a>,
    sync: bool,
    reported: Instant,
    // Each counted regular file with more than one name: its st_nlink and the names this walk saw.
    links: HashMap<Identity, (u64, u64)>,
    // A linked file the walk cannot account for: past LINKED_CAP, or a link count that moved mid-walk.
    unaccounted: bool,
}

impl Walk<'_> {
    fn saw_link(&mut self, meta: &Entry) {
        if self.unaccounted {
            return;
        }
        let key = meta.identity();
        if self.links.len() >= LINKED_CAP && !self.links.contains_key(&key) {
            self.unaccounted = true;
            self.links = HashMap::new();
            return;
        }
        let (nlink, seen) = self.links.entry(key).or_insert((meta.nlink, 0));
        *seen += 1;
        if *nlink != meta.nlink {
            self.unaccounted = true;
        }
    }

    // A name past the walk's own count (a bind mount seen twice) or one short of st_nlink both leave a name the watches may miss.
    fn linked_outside(&self) -> bool {
        self.unaccounted || self.links.values().any(|&(nlink, seen)| seen != nlink)
    }

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

pub(crate) fn walk_core<'a>(
    path: &Path,
    stop: &'a dyn Fn() -> bool,
    dev_of: DevLookup<'a>,
    progress: Option<Progress<'a>>,
) -> Tally {
    walk_from(
        path,
        stop,
        dev_of,
        progress,
        None,
        stat::probe(path, true),
        true,
    )
}

pub(super) fn walk_from<'a>(
    path: &Path,
    stop: &'a dyn Fn() -> bool,
    dev_of: DevLookup<'a>,
    progress: Option<Progress<'a>>,
    arm: Arm<'a>,
    start: Option<Entry>,
    sync: bool,
) -> Tally {
    let start_dev = start.as_ref().map(|m| m.dev);
    let start_mount = start.as_ref().and_then(|m| m.mount);
    let expected = start.as_ref().map(Entry::identity);
    let tally = match start {
        Some(meta) => Tally {
            bytes: meta.size,
            items: 0,
            partial: false,
            linked: false,
        },
        None => Tally {
            bytes: 0,
            items: 0,
            partial: true,
            linked: false,
        },
    };
    let mut walk = Walk {
        tally,
        stop,
        start_dev,
        start_mount,
        dev_of,
        progress,
        arm,
        sync,
        reported: Instant::now(),
        links: HashMap::new(),
        unaccounted: false,
    };
    if let Some(dir) = expected.and_then(|expected| open_same(path, expected)) {
        if walk_into(dir, path, &mut walk).is_none() {
            walk.tally.partial = true;
        }
    } else {
        walk.tally.partial = true;
    }
    if stat::at(path).as_ref().map(Entry::identity) != expected {
        walk.tally.partial = true;
    }
    walk.tally.linked = walk.linked_outside();
    walk.tally
}

pub(super) fn same_device(start: Option<u64>, child: u64) -> bool {
    match start {
        Some(device) => device == child,
        None => true,
    }
}

// One held descriptor at any depth, returning through each checked parent, with the folders left to visit on the heap.
fn walk_into(mut dir: File, path: &Path, walk: &mut Walk<'_>) -> Option<File> {
    let mut path = path.to_path_buf();
    // Per open folder: the folders left to visit in it, its identity, and the folder it was entered through.
    let mut open: Vec<(std::vec::IntoIter<Folder>, Identity, Option<Folder>)> = Vec::new();
    let mut entered = None;
    loop {
        let parent_identity = stat::of(&dir)?.identity();
        let mut folders = Vec::new();
        list_into(&dir, &path, walk, &mut folders);
        open.push((folders.into_iter(), parent_identity, entered.take()));
        loop {
            let level = open.last_mut()?;
            let next = match level.0.next() {
                Some(_) if (walk.stop)() => {
                    walk.tally.partial = true;
                    None
                }
                next => next,
            };
            if let Some(folder) = next {
                let at = held_path(&dir).join(&folder.name);
                let Some(child) = open_same(&at, folder.identity) else {
                    walk.tally.partial = true;
                    continue;
                };
                dir = child;
                path.push(&folder.name);
                entered = Some(folder);
                break;
            }
            let (_, _, through) = open.pop()?;
            let Some(folder) = through else {
                return Some(dir);
            };
            let parent = open_same(&held_path(&dir).join(".."), open.last()?.1);
            let Some(parent) = parent else {
                walk.tally.partial = true;
                return None;
            };
            dir = parent;
            path.pop();
            if stat::at(&held_path(&dir).join(&folder.name))
                .as_ref()
                .map(Entry::identity)
                != Some(folder.identity)
            {
                walk.tally.partial = true;
            }
        }
    }
}

fn list_into(dir: &File, path: &Path, walk: &mut Walk<'_>, folders: &mut Vec<Folder>) {
    if let Some(arm) = walk.arm {
        arm(path, &held_path(dir).join("."));
    }
    if let Some(entering) = walk.progress.as_ref().and_then(|p| p.entering) {
        entering(path);
    }
    if (walk.stop)() {
        walk.tally.partial = true;
        return;
    }
    let entries = match std::fs::read_dir(held_path(dir)) {
        Ok(entries) => entries,
        Err(_) => {
            walk.tally.partial = true;
            return;
        }
    };
    for entry in entries {
        if (walk.stop)() {
            walk.tally.partial = true;
            return;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                walk.tally.partial = true;
                continue;
            }
        };
        let name = entry.file_name();
        let named = path.join(&name);
        let known_dev = walk.dev_of.and_then(|lookup| lookup(&named));
        let crossing = known_dev.is_some_and(|device| !same_device(walk.start_dev, device));
        let entering = walk
            .progress
            .as_ref()
            .and_then(|p| p.entering)
            .filter(|_| crossing);
        if let Some(entering) = entering {
            entering(&named);
        }
        let stat = stat::child(dir, &name, walk.sync);
        if let Some(entering) = entering {
            entering(path);
        }
        let meta = match stat {
            Some(meta) => meta,
            None => {
                walk.tally.partial = true;
                continue;
            }
        };
        walk.count(meta.size);
        if meta.is_file() && meta.nlink > 1 {
            walk.saw_link(&meta);
        }
        let child_dev = known_dev.unwrap_or(meta.dev);
        let same_mount = walk
            .start_mount
            .zip(meta.mount)
            .map_or(true, |(start, child)| start == child);
        if meta.is_dir() && !meta.automount && same_device(walk.start_dev, child_dev) && same_mount
        {
            folders.push(Folder {
                name,
                identity: meta.identity(),
            });
        }
    }
}

#[cfg(test)]
#[path = "dirsize_open_tests.rs"]
mod open_tests;
