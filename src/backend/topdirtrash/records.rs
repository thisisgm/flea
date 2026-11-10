// A local Trash URI carries the backing path, so equal names on separate mounts stay distinct.
use super::*;
use std::io::{Read, Seek};

pub(crate) const PREFIX: &str = "trash:///.flea-topdir/";
// A record's Path line is at most three times PATH_MAX once encoded, so a larger file is not a record.
const MAX_RECORD_BYTES: u64 = 16384;

pub(crate) struct Record {
    pub entry: Entry,
    pub path: PathBuf,
    pub deleted: String,
    pub backing: crate::backend::trashdelete::Reviewed,
}

pub(super) fn uri(path: &Path) -> String {
    format!(
        "{PREFIX}{}",
        percent_encode(path.as_os_str().as_bytes()).replace('/', "%2F")
    )
}

pub(super) fn read(location: &locations::Location, name: &OsStr) -> Option<Record> {
    let path = location.root.join("files").join(name);
    let payload = locations::fd_path(&location.files)
        .join(name)
        .symlink_metadata()
        .ok()?;
    let record = locations::fd_path(&location.info).join(info_name(name));
    let mut file =
        crate::backend::regfile::open_if_regular(&record, crate::oflags::O_NOFOLLOW).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() > MAX_RECORD_BYTES {
        return None;
    }
    let original = crate::backend::trashdelete::original_path(&file, &location.root).ok()?;
    // A topdir record may not restore outside its volume, even when an older writer used an absolute Path.
    if !original.starts_with(&location.top) || original == location.top {
        return None;
    }
    let mut text = String::new();
    file.rewind().ok()?;
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_RECORD_BYTES {
        return None;
    }
    let direct_info = location.root.join("info").join(info_name(name));
    let same_record = || {
        direct_info.symlink_metadata().is_ok_and(|current| {
            current.dev() == metadata.dev()
                && current.ino() == metadata.ino()
                && current.len() == metadata.len()
                && current.mtime() == metadata.mtime()
                && current.mtime_nsec() == metadata.mtime_nsec()
                && current.mode() == metadata.mode()
        })
    };
    if !same_record() {
        return None;
    }
    let backing = crate::backend::trashdelete::Reviewed::inspect(
        path.clone(),
        &format!("l{}:{}", payload.dev(), payload.ino()),
    )
    .ok()?;
    if !same_record() || !backing.unchanged() {
        return None;
    }
    // Sample: "[Trash Info]\nPath=kit.zip\nDeletionDate=2026-11-10T09:12:00\n" dates the row 2026-11-10T09:12:00.
    let deleted = text
        .lines()
        .find_map(|line| line.strip_prefix("DeletionDate="))
        .unwrap_or("")
        .into();
    Some(Record {
        entry: Entry {
            original,
            uri: uri(&path),
            before: Vec::new(),
            trashed_at: String::new(),
        },
        path,
        deleted,
        backing,
    })
}

// One Trash reached through two mount points of the same filesystem lists once.
pub(crate) fn list() -> Vec<Record> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for location in locations::known() {
        let Ok(store) = location.files.metadata() else {
            continue;
        };
        if !seen.insert((store.dev(), store.ino())) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(locations::fd_path(&location.files)) else {
            continue;
        };
        out.extend(
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| read(&location, &entry.file_name())),
        );
    }
    out
}

pub(crate) fn resolve(uri: &str) -> Option<Record> {
    resolver()(uri)
}

// Opens the known Trash directories on the first mount entry only, then reuses them for every later one.
pub(crate) fn resolver() -> impl Fn(&str) -> Option<Record> {
    let known = std::cell::OnceCell::new();
    move |uri| {
        if !uri.starts_with(PREFIX) {
            return None;
        }
        resolve_in(known.get_or_init(locations::known), uri)
    }
}

fn resolve_in(known: &[locations::Location], uri: &str) -> Option<Record> {
    let path = PathBuf::from(
        <std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(
            crate::paths::percent_decode_bytes(uri.strip_prefix(PREFIX)?),
        ),
    );
    if path.components().any(|part| {
        matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) {
        return None;
    }
    let name = path.file_name()?;
    known.iter().find_map(|location| {
        (path.parent() == Some(location.root.join("files").as_path()))
            .then(|| read(location, name))
            .flatten()
    })
}
