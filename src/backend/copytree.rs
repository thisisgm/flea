// A tree copied from a stack on the heap, so neither stack nor descriptors grow with its depth; see deeptree.rs.
use super::deep::{self, Chain};
use super::{
    cancel_err, cancelled, copy_file_at, drop_folder, held_path, keep_mtime, mode_for_source,
    open_dir, remove_tree, touch, At, Progress,
};
use crate::backend::copystage::Folder;
use crate::backend::unheld::{self, Kind};
use crate::error::{from_io, FleaError};
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

// What is left to copy in one folder: streamed while its descriptors are held, listed once they are let go.
enum Entries {
    Open(std::fs::ReadDir),
    Listed(std::vec::IntoIter<io::Result<(OsString, bool)>>),
}

impl Entries {
    // The name and whether the listing already calls it a regular file, which skips the lstat.
    fn next(&mut self) -> Option<io::Result<(OsString, bool)>> {
        match self {
            Entries::Open(read) => read.next().map(|entry| {
                entry.map(|e| {
                    (
                        e.file_name(),
                        e.file_type().map(|t| t.is_file()).unwrap_or(false),
                    )
                })
            }),
            Entries::Listed(rest) => rest.next(),
        }
    }

    // Its descriptor is about to go, so the rest of the listing is read now; a read error waits at its place.
    fn list(&mut self) {
        if let Entries::Open(_) = self {
            let mut rest = Vec::new();
            while let Some(entry) = self.next() {
                let failed = entry.is_err();
                rest.push(entry);
                if failed {
                    break;
                }
            }
            *self = Entries::Listed(rest.into_iter());
        }
    }
}

// One folder below the top of the copy: what is left to read and the mode it takes once its children landed.
struct Level {
    entries: Entries,
    keep: Option<u32>,
}

struct Walk {
    from: Chain,
    into: Chain,
    src: PathBuf,
    dst: PathBuf,
    top: Level,
    levels: Vec<Level>,
}

pub(super) fn copy_dir_at(
    src: At,
    dst: At,
    p: &mut Progress,
    inside: bool,
) -> Result<(), FleaError> {
    // Issue 110: a child is reached through held descriptors, or past the window through ".." checked by identity, so a parent renamed aside cannot redirect the tree.
    let from = open_dir(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
    // A move removes every folder's contents once the copy lands, so a folder that cannot give them up fails now.
    if p.for_move {
        writable(&from, src.named)?;
    }
    // Issue 109: a folder is created with no bit the source lacks plus the owner's three this run writes with, and takes its exact mode at the end.
    let source_magic = crate::backend::fsinfo::magic_of(src.at);
    let keep = from
        .metadata()
        .ok()
        .map(|m| mode_for_source(source_magic, m.permissions().mode(), true));
    // A new folder is built under a hidden sibling and takes its name in one rename; see copystage.rs.
    let folder = unheld::fault(dst.named, Kind::Entry)
        .and_then(|()| Folder::create(dst.at, keep.unwrap_or(0o700) | 0o700, inside))
        .map_err(|e| unheld::refused(Kind::Entry, &e, dst, p.tree.is_some().then_some(src.at)))?;
    // A staged folder is still empty here, so it goes at once rather than wait for a sweep after this backend.
    let into = open_dir(folder.at())
        .inspect_err(|_| {
            if folder.staged() {
                let _ = std::fs::remove_dir(folder.at());
            }
        })
        .map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))?;
    super::super::tags::copy_tags(src.at, folder.at());
    record_folder(p, dst.named, &into);
    // Set once at the top of the tree, so a directory inside it goes on counting rather than starting again.
    if p.tree.is_none() {
        p.tree = Some(0);
    }
    let entries = std::fs::read_dir(held_path(&from))
        .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e));
    // Only the top folder is ever staged: everything below it lands inside the hidden sibling, or in place where the drive writes in place.
    let (from, into, r) = match entries {
        Ok(entries) => {
            let mut walk = Walk {
                from: Chain::new(from),
                into: Chain::new(into),
                src: src.named.to_path_buf(),
                dst: dst.named.to_path_buf(),
                top: Level {
                    entries: Entries::Open(entries),
                    keep: None,
                },
                levels: Vec::new(),
            };
            let r = walk.run(p);
            (walk.from.into_base(), walk.into.into_base(), r)
        }
        Err(e) => (from, into, Err(e)),
    };
    if let Err(e) = r {
        if folder.staged() {
            return Err(drop_folder(p, dst, &folder, e));
        }
        if cancelled(p) {
            // A half-copied tree goes with the cancel, gated on the flag not the message.
            p.partial = match remove_tree(dst.at) {
                Ok(()) => None,
                // Still there, so the journal is told where it is rather than that nothing was left.
                Err(()) => Some(dst.named.to_path_buf()),
            };
            if p.partial.is_none() {
                if let Some(durability) = p.durability.as_mut() {
                    durability.forget_tree(dst.named);
                }
            }
        } else {
            // Any other failure may be transient, so what was copied stays and the journal records the tree as its one partial.
            p.partial = Some(dst.named.to_path_buf());
        }
        return Err(e);
    }
    settle(&from, &into, keep);
    if let Err(e) = folder.publish(dst.at) {
        let refused = unheld::refused(Kind::Entry, &e, dst, p.tree.is_some().then_some(src.at));
        return Err(drop_folder(p, dst, &folder, refused));
    }
    touch(p, dst.named);
    if let Some(parent) = dst.named.parent() {
        touch(p, parent);
    }
    Ok(())
}

impl Walk {
    fn run(&mut self, p: &mut Progress) -> Result<(), FleaError> {
        let rename_fat = p.durability.as_ref().is_some_and(|d| d.rename_fat);
        loop {
            if cancelled(p) {
                return Err(cancel_err(&self.dst));
            }
            let next = self.top.entries.next();
            let Some(next) = next else {
                if self.levels.is_empty() {
                    return Ok(());
                }
                self.leave(p)?;
                continue;
            };
            let (name, is_file) =
                next.map_err(|e| from_io("copy", &self.src.to_string_lossy(), &e))?;
            // The card's Rename answer reaches every level, so a name the drive refuses lands under the name it gives a top-level one.
            let landed = crate::backend::fatnames::inside_leaf(&name, rename_fat);
            let (from_at, into_at) = (
                held_path(self.from.top()).join(&name),
                held_path(self.into.top()).join(&landed),
            );
            self.src.push(&name);
            self.dst.push(&landed);
            let (from_named, into_named) = (self.src.clone(), self.dst.clone());
            let (src, dst) = (
                At {
                    at: &from_at,
                    named: &from_named,
                },
                At {
                    at: &into_at,
                    named: &into_named,
                },
            );
            let outcome = if is_file {
                // The total is unused inside a tree: p.tree is Some here.
                copy_file_at(src, dst, 0, p, true)
            } else {
                match src.at.symlink_metadata() {
                    Ok(meta) if meta.is_dir() => match self.enter(src, dst, &name, &landed, p) {
                        // The new level is the top now, and the two names stay pushed until it is left.
                        Ok(()) => continue,
                        Err(e) => Err(e),
                    },
                    Ok(_) => super::copy_at(src, dst, p, true),
                    Err(e) => Err(from_io("copy", &src.named.to_string_lossy(), &e)),
                }
            };
            self.src.pop();
            self.dst.pop();
            // An item the drive cannot hold is named and skipped, so one link or one FAT name never ends a tree.
            match outcome {
                Err(e) if e.where_ == unheld::UNHELD => unheld::note(e),
                Err(e) => return Err(e),
                Ok(()) => {}
            }
        }
    }

    // The same steps the top folder takes, in place, since the top folder already hides everything below it.
    fn enter(
        &mut self,
        src: At,
        dst: At,
        name: &OsString,
        landed: &OsString,
        p: &mut Progress,
    ) -> Result<(), FleaError> {
        let from =
            open_dir(src.at).map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
        if p.for_move {
            writable(&from, src.named)?;
        }
        let source_magic = crate::backend::fsinfo::magic_of(src.at);
        let keep = from
            .metadata()
            .ok()
            .map(|m| mode_for_source(source_magic, m.permissions().mode(), true));
        let folder = unheld::fault(dst.named, Kind::Entry)
            .and_then(|()| Folder::create(dst.at, keep.unwrap_or(0o700) | 0o700, true))
            .map_err(|e| unheld::refused(Kind::Entry, &e, dst, Some(src.at)))?;
        let into =
            open_dir(folder.at()).map_err(|e| from_io("copy", &dst.named.to_string_lossy(), &e))?;
        super::super::tags::copy_tags(src.at, folder.at());
        record_folder(p, dst.named, &into);
        let entries = std::fs::read_dir(held_path(&from))
            .map_err(|e| from_io("copy", &src.named.to_string_lossy(), &e))?;
        let held = |e: io::Error| from_io("copy", &src.named.to_string_lossy(), &e);
        let let_go = self.from.push_open(name, from).map_err(held)?;
        self.into.push_open(landed, into).map_err(held)?;
        // Both chains let the same level go; that folder's listing is read now, since its descriptor goes with it.
        if let Some(level) = let_go {
            self.levels[level + 1].entries.list();
        }
        let parent = std::mem::replace(
            &mut self.top,
            Level {
                entries: Entries::Open(entries),
                keep,
            },
        );
        self.levels.push(parent);
        Ok(())
    }

    // A finished folder takes its mode and mtime, and the walk climbs back to its parent through "..".
    fn leave(&mut self, p: &mut Progress) -> Result<(), FleaError> {
        let Some(parent) = self.levels.pop() else {
            return Err(from_io(
                "copy",
                &self.src.to_string_lossy(),
                &io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the copy walk has no parent folder",
                ),
            ));
        };
        let level = std::mem::replace(&mut self.top, parent);
        // The climb goes first, so a copy that takes a mode without the owner's search bit still lets ".." through.
        let climbed = self
            .from
            .pop()
            .and_then(|from| Ok((from, self.into.pop()?)));
        let (from, into) = climbed.map_err(|e| from_io("copy", &self.src.to_string_lossy(), &e))?;
        settle(&from, &into, level.keep);
        touch(p, &self.dst);
        if let Some(parent) = self.dst.parent() {
            touch(p, parent);
        }
        self.src.pop();
        self.dst.pop();
        Ok(())
    }
}

// Last, so a folder still being written stays writable; a destination that refuses the mode keeps the widened one, since every byte landed.
fn settle(from: &File, into: &File, keep: Option<u32>) {
    if let Some(mode) = keep {
        let _ = std::fs::set_permissions(held_path(into), std::fs::Permissions::from_mode(mode));
    }
    // The directory carries the source mtime too, after its children landed inside it.
    if let Ok(meta) = from.metadata() {
        keep_mtime(into, &meta);
    }
}

fn record_folder(p: &mut Progress, named: &Path, into: &File) {
    if let Some(writer) = p.manifest.as_mut() {
        match into.metadata() {
            Ok(meta) => writer.record(named, &meta),
            Err(_) => writer.overflow(),
        }
    }
}

// A folder whose entries this process may not remove keeps a move from starting on it.
fn writable(dir: &File, named: &Path) -> Result<(), FleaError> {
    deep::writable(&held_path(dir)).map_err(|e| from_io("move", &named.to_string_lossy(), &e))
}
