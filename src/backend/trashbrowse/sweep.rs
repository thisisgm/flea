// Empty Trash also removes what the listing leaves out, as gio trash --empty does, so Empty means empty.
use super::*;
use crate::backend::trashdelete::PathReview;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

// A record's identity at review; the sweep removes it only while it is still that file.
#[derive(Clone, Debug, PartialEq)]
struct Stamp([i64; 7]);
fn stamp(path: &Path) -> Option<Stamp> {
    let m = path.symlink_metadata().ok().filter(|meta| meta.is_file())?;
    Some(Stamp([
        m.dev() as i64,
        m.ino() as i64,
        m.size() as i64,
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    ]))
}
fn gone(path: &Path) -> bool {
    matches!(path.symlink_metadata(), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

// A files entry the listing left out (no record, or one gio cannot describe), reviewed like a permanent delete.
struct Payload {
    review: PathReview,
    path: PathBuf,
    record: Option<(PathBuf, Stamp)>,
}
// A record whose item is gone; gio leaves it behind, and the sweep removes it while the item stays gone.
struct Orphan {
    path: PathBuf,
    item: PathBuf,
    root: PathBuf,
    stamp: Stamp,
}
#[derive(Default)]
pub(super) struct Sweep {
    payloads: Vec<Payload>,
    orphans: Vec<Orphan>,
}

// Sample input, gio list -u -h -a standard::target-uri trash:///: "trash:///a%20b\t1\t(regular)\tstandard::target-uri=file:///h/.local/share/Trash/files/a%20b\n".
fn targets(text: &str) -> Vec<(&str, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let uri = fields.next()?;
            let path =
                fields.find_map(|field| field.strip_prefix("standard::target-uri=file://"))?;
            Some((
                uri,
                PathBuf::from(OsString::from_vec(crate::paths::percent_decode_bytes(path))),
            ))
        })
        .collect()
}
// The Trash folder holding a backing file at `<root>/files/<name>` beside `<root>/info`; a claim's own quarantine is never swept.
fn root_of(path: &Path) -> Option<&Path> {
    let name = path.file_name()?;
    let files = path.parent()?;
    let root = files.parent()?;
    let swept = path.is_absolute()
        && !name.as_bytes().starts_with(b".flea-delete-")
        && files.file_name() == Some(OsStr::new("files"));
    (swept && root.join("info").is_dir()).then_some(root)
}
fn record_of(root: &Path, name: &OsStr) -> PathBuf {
    root.join("info")
        .join(crate::backend::hometrash::info_name(name))
}
// A claim left stuck can hold its item in a quarantine while the record is back in info, so that Trash keeps its records.
fn claimed(root: &Path) -> Option<&'static str> {
    let Ok(entries) = std::fs::read_dir(root.join("files")) else {
        return Some("its Trash's files folder cannot be read");
    };
    entries
        .flatten()
        .any(|entry| entry.file_name().as_bytes().starts_with(b".flea-delete-"))
        .then_some("a claim's quarantine is in its Trash")
}
fn skipped(what: &Path, why: impl std::fmt::Display) {
    crate::backend::trashmanifest::log(&format!(
        "Empty Trash left {} in place: {why}",
        what.display()
    ));
}

impl Sweep {
    // A leftover that cannot be reviewed is logged and left, so it never holds back the items the person reviewed.
    pub(super) fn review(
        listed: &[Item],
        trees: &mut Manifest,
        scratch: &Path,
        cancel: &Cancellation,
        read: impl Fn(&[&str]) -> Result<String, Refusal>,
    ) -> Result<Self, String> {
        let listed = listed_uris(listed);
        let mut roots: BTreeSet<PathBuf> = crate::backend::hometrash::home_trash_root()
            .into_iter()
            .collect();
        let mut seen = HashSet::new();
        let mut sweep = Self::default();
        let text = read(&[
            "list",
            "-u",
            "-h",
            "-a",
            "standard::target-uri",
            "trash:///",
        ])
        .unwrap_or_else(|refusal| {
            crate::backend::trashmanifest::log(&format!(
                "Empty Trash could not list the Trash's backing files through gio: {}",
                refusal.shown()
            ));
            String::new()
        });
        for (uri, path) in targets(&text) {
            cancel.check()?;
            let Some(root) = root_of(&path) else { continue };
            roots.insert(root.to_path_buf());
            if listed.contains(uri) || !seen.insert(path.clone()) {
                continue;
            }
            let record = record_of(root, path.file_name().unwrap_or_default());
            match PathReview::prepare(path.clone(), trees, scratch, cancel) {
                Ok((review, _)) => sweep.payloads.push(Payload {
                    review,
                    record: stamp(&record).map(|s| (record, s)),
                    path,
                }),
                Err(refusal) => skipped(&path, refusal.shown()),
            }
        }
        for root in roots {
            cancel.check()?;
            if let Some(why) = claimed(&root) {
                skipped(&root.join("info"), why);
                continue;
            }
            let Ok(entries) = std::fs::read_dir(root.join("info")) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(stem) = name
                    .as_bytes()
                    .strip_suffix(b".trashinfo")
                    .filter(|stem| !stem.is_empty())
                else {
                    continue;
                };
                let (path, item) = (
                    entry.path(),
                    root.join("files").join(OsStr::from_bytes(stem)),
                );
                if let (true, Some(stamp)) = (gone(&item), stamp(&path)) {
                    sweep.orphans.push(Orphan {
                        path,
                        item,
                        root: root.clone(),
                        stamp,
                    });
                }
            }
        }
        Ok(sweep)
    }
    pub(super) fn unchanged(&self, cancel: &Cancellation) -> Result<bool, Refusal> {
        for payload in &self.payloads {
            if !payload.review.unchanged(cancel)? {
                return Ok(false);
            }
            if payload
                .record
                .as_ref()
                .is_some_and(|(path, was)| stamp(path).as_ref() != Some(was))
            {
                return Ok(false);
            }
        }
        Ok(self.orphans.iter().all(|orphan| {
            gone(&orphan.item) && stamp(&orphan.path).as_ref() == Some(&orphan.stamp)
        }))
    }
    // Runs after the reviewed items; what fails is logged, since the person never saw these entries.
    pub(super) fn remove(
        &self,
        recovery_root: &Path,
        cancel: &Cancellation,
        thumbnails: &mut ThumbnailPurge,
    ) {
        for payload in &self.payloads {
            if cancel.check().is_err() {
                return;
            }
            if let Err(refusal) = payload.review.delete(recovery_root) {
                skipped(&payload.path, refusal.shown());
                continue;
            }
            thumbnails
                .0
                .push(crate::backend::thumbcache::uri_for(&payload.path));
            if let Some((record, was)) = &payload.record {
                if stamp(record).as_ref() == Some(was) {
                    if let Err(e) = std::fs::remove_file(record) {
                        skipped(record, e);
                    }
                }
            }
        }
        for orphan in &self.orphans {
            if cancel.check().is_err() {
                return;
            }
            if !gone(&orphan.item) || stamp(&orphan.path).as_ref() != Some(&orphan.stamp) {
                skipped(&orphan.path, "it changed");
                continue;
            }
            if let Some(why) = claimed(&orphan.root) {
                skipped(&orphan.path, why);
                continue;
            }
            if let Err(e) = std::fs::remove_file(&orphan.path) {
                skipped(&orphan.path, e);
            }
        }
    }
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
