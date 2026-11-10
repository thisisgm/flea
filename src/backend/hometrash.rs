// Issue 261: Replace on a location with no Trash keeps the old item in the home Trash.
use crate::backend::trash::Entry;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// No GVFS backend implements trash, so gio trash refuses every path under its FUSE root /run/user/<uid>/gvfs (sample: .../gvfs/smb-share:server=192.168.21.25,share=data/photos); a folder named gvfs elsewhere is plain.
pub fn trashable(path: &Path) -> bool {
    match crate::backend::extclass::gvfs_root(path) {
        Some(share) => !names_a_mount(&share.file_name().unwrap_or_default().to_string_lossy()),
        None => true,
    }
}

// Sample input: "mtp:host=SAMSUNG_Android", a type of letters, digits and hyphens before the first colon.
fn names_a_mount(name: &str) -> bool {
    match name.find(':') {
        Some(end) if end > 0 => name[..end].chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        _ => false,
    }
}

// Encodes a Trash file name or path for its URI or record: every raw byte outside the unreserved set rides as one uppercase %XX triplet, with / kept literal.
pub(crate) fn percent_encode(s: &[u8]) -> String {
    const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~/";
    let mut out = String::with_capacity(s.len());
    for b in s {
        if UNRESERVED.contains(b) {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// The home Trash gio restores from: the test override under cfg(test), else the data home's Trash.
#[cfg(test)]
thread_local! {
    static TRASH_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

// Points one test thread's home Trash at its own sandbox; no test may move anything into a Trash named by the process env.
#[cfg(test)]
pub(crate) fn set_test_trash_root(root: Option<PathBuf>) {
    TRASH_ROOT.with(|slot| *slot.borrow_mut() = root);
}

// Reads the override first, so a test without one refuses before any mkdir rather than touching a real Trash.
pub(crate) fn home_trash_root() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(root) = TRASH_ROOT.with(|slot| slot.borrow().clone()) {
        return Some(root);
    }
    #[cfg(test)]
    return None;
    #[cfg(not(test))]
    return crate::userfile::data_home().ok().map(|home| home.join("Trash"));
}

// The whole sentence a home Trash move that failed for any reason refuses with.
pub(crate) const HOME_TRASH_FAILED: &str = "the item already there could not be moved to the home Trash, so nothing was replaced";
// The whole sentence a home Trash move refuses with when the home Trash's filesystem has no room left.
pub(crate) const HOME_TRASH_FULL: &str = "there is not enough room in the home Trash for the item already there, so nothing was replaced";
// The whole sentence a home Trash move refuses with when the original folder came away only in part.
pub(crate) const HOME_TRASH_PARTIAL: &str = "the folder was only partly removed from its place, and its complete copy is in the home Trash";

// Names the refusal for a failed copy once the free room on the home Trash's filesystem is known.
pub(crate) fn sentence_after_copy(free_bytes: u64) -> &'static str {
    if free_bytes == 0 { HOME_TRASH_FULL } else { HOME_TRASH_FAILED }
}

// Names the refusal for a filesystem call that failed directly, where the errno is in hand.
pub(crate) fn sentence_for_io(error: &std::io::Error) -> &'static str {
    const ENOSPC: i32 = 28;
    if error.raw_os_error() == Some(ENOSPC) { HOME_TRASH_FULL } else { HOME_TRASH_FAILED }
}

pub(crate) use crate::backend::trashdate::deletion_date_local;

#[cfg(test)]
thread_local! {
    static TEST_FREE: std::cell::RefCell<Option<(std::path::PathBuf, u64)>> = const { std::cell::RefCell::new(None) };
}

// Test seam: free_bytes answers `free` for exactly `dir`, and the real statfs for every other path.
#[cfg(test)]
pub(crate) fn test_set_free(at: Option<(std::path::PathBuf, u64)>) {
    TEST_FREE.with(|v| *v.borrow_mut() = at);
}

// Reads the free room on the home Trash's filesystem through the one statfs reader, so a failed copy on a full one says so.
fn free_bytes(dir: &Path) -> u64 {
    #[cfg(test)]
    if let Some(free) = TEST_FREE.with(|v| v.borrow().as_ref().filter(|(at, _)| at == dir).map(|(_, f)| *f)) {
        return free;
    }
    crate::backend::fsinfo::read(dir).map(|info| info.free).unwrap_or(u64::MAX)
}

// Suffixes 2 up to one below this are tried; past them every Trash name counts as taken, so the reserve gives up instead of looping for good.
const SUFFIX_LIMIT: usize = 1000;
// Names the first free Trash file name: the item's own name, then stem.2.ext past a collision.
fn candidate_names(file_name: &OsStr) -> Vec<OsString> {
    let plain = file_name.as_bytes();
    let mut out = vec![file_name.to_os_string()];
    let (stem, ext) = match plain.iter().rposition(|b| *b == b'.').filter(|dot| *dot > 0) {
        Some(dot) => plain.split_at(dot),
        None => (plain, &b""[..]),
    };
    for n in 2..SUFFIX_LIMIT {
        out.push(OsString::from_vec([stem, format!(".{n}").as_bytes(), ext].concat()));
    }
    out
}

// Confirms every byte the copy laid down, files and the folders holding them, before the original goes.
fn sync_tree(root: &Path) -> std::io::Result<()> {
    let meta = root.symlink_metadata()?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        for entry in std::fs::read_dir(root)? {
            sync_tree(&entry?.path())?;
        }
    }
    // A fifo's open waits for a writer, so only files and folders are synced.
    if meta.is_file() || meta.is_dir() {
        std::fs::File::open(root)?.sync_all()?;
    }
    Ok(())
}

// Makes the home Trash's two folders owner-only, the way gio does on first use.
pub(crate) fn trash_dirs(root: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let (files, info) = (root.join("files"), root.join("info"));
    for dir in [&files, &info] {
        std::fs::DirBuilder::new().mode(0o700).recursive(true).create(dir)?;
    }
    Ok((files, info))
}

// Reserves the first free Trash name: its info file created exclusively and written, synced when asked, and no files entry of that name.
pub(crate) fn reserve(files: &Path, info: &Path, original: &Path, name: &OsStr, sync: bool) -> std::io::Result<(OsString, PathBuf)> {
    for candidate in candidate_names(name) {
        let info_path = info.join(info_name(&candidate));
        let mut info_file = match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&info_path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        // A files entry this call did not create is another item's, so its name is taken too.
        if files.join(&candidate).symlink_metadata().is_ok() {
            let _ = std::fs::remove_file(&info_path);
            continue;
        }
        let body = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", percent_encode(original.as_os_str().as_bytes()), deletion_date_local());
        if let Err(e) = std::io::Write::write_all(&mut info_file, body.as_bytes()).and_then(|()| if sync { sync_record(&info_file) } else { Ok(()) }) {
            let _ = std::fs::remove_file(&info_path);
            return Err(e);
        }
        return Ok((candidate, info_path));
    }
    Err(std::io::Error::other("every Trash name is taken"))
}

#[cfg(test)]
thread_local! {
    static RECORD_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// Test seam: how many info files were synced one by one since the last read.
#[cfg(test)]
pub(crate) fn test_take_record_syncs() -> usize {
    RECORD_SYNCS.with(|v| v.replace(0))
}

// Confirms one info file's bytes on the drive before its item moves.
fn sync_record(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(test)]
    RECORD_SYNCS.with(|v| v.set(v.get() + 1));
    file.sync_all()
}

// A Trash file name's record name, raw bytes kept.
pub(crate) fn info_name(name: &OsStr) -> OsString {
    let mut out = name.to_os_string();
    out.push(".trashinfo");
    out
}

// The entry a home Trash move answers, under the file name it took.
fn home_entry(original: &Path, candidate: &OsStr) -> Entry {
    Entry { original: original.to_path_buf(), uri: format!("trash:///{}", percent_encode(candidate.as_bytes())), before: Vec::new(), trashed_at: String::new() }
}

// Moves one replaced item into the home Trash the freedesktop way and answers its trash:/// entry.
pub(crate) fn trash_to_home(original: &Path, cancel: &AtomicBool) -> Result<Entry, String> {
    // A cancel that landed before the copy starts costs no Trash entry at all.
    if cancel.load(Ordering::Relaxed) {
        return Err(crate::backend::collide::CANCELLED.to_string());
    }
    let Some(root) = home_trash_root() else { return Err(HOME_TRASH_FAILED.to_string()) };
    let (files, info) = trash_dirs(&root).map_err(|e| sentence_for_io(&e).to_string())?;
    let Some(name) = original.file_name() else { return Err(HOME_TRASH_FAILED.to_string()) };
    let (candidate, info_path) = reserve(&files, &info, original, name, true).map_err(|e| sentence_for_io(&e).to_string())?;
    let files_path = files.join(&candidate);
    if let Err(reason) = place_copy(original, &files_path, &files, cancel) {
        drop_orphan(&files_path, &info_path);
        return Err(reason);
    }
    if let Err(e) = sync_tree(&files_path).and(sync_dir(&files)).and(sync_dir(&info)) {
        drop_orphan(&files_path, &info_path);
        return Err(sentence_for_io(&e).to_string());
    }
    take_original(original, &files_path, &info_path)?;
    Ok(home_entry(original, &candidate))
}

#[cfg(test)]
thread_local! {
    static OTHER_FS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Test seam: every item reads as sitting on another filesystem than the home Trash.
#[cfg(test)]
pub(crate) fn test_set_other_filesystem(on: bool) {
    OTHER_FS.with(|v| v.set(on));
}

// Compares the item's folder with the home Trash, or the nearest folder above it that exists yet.
fn same_filesystem(original: &Path, root: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    #[cfg(test)]
    if OTHER_FS.with(|v| v.get()) {
        return false;
    }
    let Some(parent) = original.parent().and_then(|p| p.metadata().ok()) else { return false };
    // GLib reads the item's own device, so a mounted or nested volume under a shared parent still goes to gio.
    if original.symlink_metadata().is_ok_and(|own| own.dev() != parent.dev()) {
        return false;
    }
    root.ancestors().find_map(|at| at.metadata().ok()).is_some_and(|trash| trash.dev() == parent.dev())
}

// The home Trash and the item's file name when the item may be renamed into it in-process; an item inside the Trash itself, or on another filesystem, is left to gio.
fn rename_target(original: &Path) -> Option<(PathBuf, &OsStr)> {
    let root = home_trash_root()?;
    let name = original.file_name()?;
    // A record's Path= is absolute in the home Trash, so a relative path is gio's too.
    if !original.is_absolute() || original.starts_with(&root) || !same_filesystem(original, &root) {
        return None;
    }
    Some((root, name))
}

// Whether rename_to_home would try this item, so a batch knows before it moves anything which paths still need gio.
pub(crate) fn renames_to_home(original: &Path) -> bool {
    rename_target(original).is_some()
}

#[cfg(test)]
thread_local! {
    static VANISH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

// Test seam: this path is deleted as a batch starts to rename it, after the batch has already counted it as renameable.
#[cfg(test)]
pub(crate) fn test_vanish_on_rename(path: Option<PathBuf>) {
    VANISH.with(|slot| *slot.borrow_mut() = path);
}

// Renames one item into the home Trash on its own filesystem, as gio does there; None means nothing moved and gio still runs.
pub(crate) fn rename_to_home(original: &Path, unsynced: &mut std::collections::BTreeSet<PathBuf>, durable: &mut Option<bool>) -> Option<Result<Entry, String>> {
    let (root, name) = rename_target(original)?;
    #[cfg(test)]
    if VANISH.with(|slot| slot.borrow().as_deref() == Some(original)) {
        let _ = std::fs::remove_file(original);
    }
    let (files, info) = trash_dirs(&root).ok()?;
    // A drive Flea confirms syncs each record before its item moves; a fixed disk writes it as gio does, unsynced.
    let confirm = *durable.get_or_insert_with(|| original.parent().is_some_and(crate::backend::durable::dest_is_durable));
    let (candidate, info_path) = reserve(&files, &info, original, name, confirm).ok()?;
    if let Err(e) = crate::backend::renamecompat::rename_noreplace(original, &files.join(&candidate)) {
        let _ = std::fs::remove_file(&info_path);
        // An item gone before its move is the Issue 88 refusal; any other failure (EXDEV, EBUSY, a taken name) keeps gio.
        let gone = e.kind() == std::io::ErrorKind::NotFound && original.symlink_metadata().is_err();
        return gone.then(|| Err(crate::backend::collide::TRASH_REFUSED.to_string()));
    }
    // The batch syncs the three folders once at its end, not once per item.
    unsynced.extend([files, info].into_iter().chain(original.parent().map(Path::to_path_buf)));
    Some(Ok(home_entry(original, &candidate)))
}

#[cfg(test)]
thread_local! {
    static FOLDER_SYNCS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

// Test seam: how many batches synced their folders, and how many folders they synced in all, since the last read.
#[cfg(test)]
pub(crate) fn test_take_folder_syncs() -> (usize, usize) {
    FOLDER_SYNCS.with(|v| v.replace((0, 0)))
}

// Syncs every folder a batch of renames left, once each; the entries are real already, so a failed sync is ignored.
pub(crate) fn sync_folders(unsynced: &mut std::collections::BTreeSet<PathBuf>) {
    if unsynced.is_empty() {
        return;
    }
    #[cfg(test)]
    FOLDER_SYNCS.with(|v| v.set((v.get().0 + 1, v.get().1 + unsynced.len())));
    for dir in std::mem::take(unsynced) {
        let _ = sync_dir(&dir);
    }
}

// Copies the original into the Trash files folder whole, a symlink as a link and never followed.
fn place_copy(original: &Path, files_path: &Path, room: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let mut sink = |_: u64, _: u64| {};
    let mut progress = crate::backend::copyfile::Progress { cancel, on_bytes: &mut sink, tree: None,
        partial: None, manifest: None, durability: None, for_move: false };
    match crate::backend::copyfile::copy_any(original, files_path, &mut progress) {
        Ok(()) => Ok(()),
        // A cancel keeps its own word, which is what the transfer counts it by; copy_any already removed the partial.
        Err(_) if cancel.load(Ordering::Relaxed) => Err(crate::backend::collide::CANCELLED.to_string()),
        // The room is read off the Trash folder itself, which stands whatever the copy left behind.
        Err(_) => Err(sentence_after_copy(free_bytes(room)).to_string()),
    }
}

// Confirms a Trash folder itself, so the entry above names bytes the drive already holds.
pub(crate) fn sync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

// Deletes a Trash copy and its info file, which is safe only because the original still stands.
fn drop_orphan(files_path: &Path, info_path: &Path) {
    let _ = std::fs::remove_file(info_path);
    match files_path.symlink_metadata() {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            let _ = std::fs::remove_dir_all(files_path);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(files_path);
        }
        Err(_) => {}
    }
}

// Removes the original only after its copy synced, keeping a half-removed folder's copy for its owner.
fn take_original(original: &Path, files_path: &Path, info_path: &Path) -> Result<(), String> {
    let is_dir = original.symlink_metadata().map(|m| m.is_dir() && !m.file_type().is_symlink()).unwrap_or(false);
    let removed = if is_dir { std::fs::remove_dir_all(original).map(|_| ()) } else { std::fs::remove_file(original) };
    match removed {
        Ok(()) => Ok(()),
        Err(e) if !is_dir && original.symlink_metadata().is_ok() => {
            drop_orphan(files_path, info_path);
            Err(sentence_for_io(&e).to_string())
        }
        // A folder that came away only in part keeps its complete Trash copy, which stays restorable.
        Err(_) if is_dir && original.symlink_metadata().is_ok() => Err(HOME_TRASH_PARTIAL.to_string()),
        Err(e) => Err(sentence_for_io(&e).to_string()),
    }
}

#[cfg(test)]
#[path = "hometrash_tests.rs"]
pub(crate) mod tests;
