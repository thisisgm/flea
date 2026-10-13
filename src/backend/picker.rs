// Picker checks retain reviewed objects; only the application receiving a URI writes its contents.
use super::listing::Listing;
use super::mime::Db;
use super::opsreq::OpMsg;
use super::trashmanifest::Cancellation;
use super::undo::ItemIdentity;
use crate::error::io_message;
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};
use std::collections::HashSet;
use std::ffi::CString;
use std::fs::{File, Metadata, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Sender, SyncSender, TrySendError};

#[path = "picker_limits.rs"]
mod limits;
#[path = "picker_folder.rs"]
mod folder;

const O_PATH: i32 = 0o10000000;
const FNM_CASEFOLD: i32 = 1 << 4;
extern "C" {
    fn fnmatch(pattern: *const std::os::raw::c_char, name: *const std::os::raw::c_char, flags: i32) -> i32;
}

pub struct Picker {
    requests: SyncSender<String>,
    replies: Sender<OpMsg>,
    cancellation: Cancellation,
}
impl Picker {
    pub fn new(replies: Sender<OpMsg>) -> Self {
        let (requests, receiver) = sync_channel::<String>(1);
        let output = replies.clone();
        let cancellation = Cancellation::default();
        let active = cancellation.clone();
        std::thread::spawn(move || {
            let mut state = State::default();
            while let Ok(line) = receiver.recv() {
                if active.check().is_err() { break; }
                let result = state.handle(&line, &active);
                if active.check().is_err() { break; }
                if output.send(OpMsg::Meta { line: response(&line, result) }).is_err() { break; }
            }
        });
        Self { requests, replies, cancellation }
    }
    pub fn request(&mut self, line: String) {
        if field_str(&line, "op").as_deref() == Some("close") {
            self.cancellation.next();
            let _ = self.requests.try_send(line);
            return;
        }
        if let Err(error) = self.requests.try_send(line) {
            let (line, message) = match error {
                TrySendError::Full(line) => (line, "Flea is still checking this item; try again."),
                TrySendError::Disconnected(line) => (line, "The check stopped; open the file chooser again."),
            };
            let _ = self.replies.send(OpMsg::Meta { line: response(&line, Err(message.into())) });
        }
    }
}
impl Drop for Picker {
    fn drop(&mut self) { self.cancellation.next(); }
}

struct Held {
    path: PathBuf,
    file: File,
    follow: bool,
    target: Option<File>,
}
impl Held {
    fn open(path: &Path, follow: bool) -> Result<Self, String> {
        if !path.is_absolute() { return Err("Picker paths must be absolute.".into()); }
        let file = OpenOptions::new().read(true).custom_flags(O_PATH | if follow { 0 } else { crate::oflags::O_NOFOLLOW })
            .open(path).map_err(|error| format!("Could not inspect {}: {}.", path.display(), io_message(&error)))?;
        Ok(Self { path: path.into(), file, follow, target: None })
    }
    fn is_dir(&self) -> Result<bool, String> {
        let metadata = self.current()?;
        match &self.target {
            Some(target) => target_is_dir(target, &self.path),
            None => Ok(metadata.is_dir()),
        }
    }
    fn current(&self) -> Result<Metadata, String> {
        let before = self.file.metadata().map_err(|error| io_message(&error))?;
        let now = if self.follow { self.path.metadata() } else { self.path.symlink_metadata() }
            .map_err(|_| "Selected item moved or disappeared.".to_string())?;
        if !ItemIdentity::record(&before).same_item(&ItemIdentity::record(&now)) {
            return Err("Selected item changed; select it again.".into());
        }
        if let Some(target) = &self.target {
            let before = target.metadata().map_err(|error| io_message(&error))?;
            let now = self.path.metadata().map_err(|_| "Selected link target moved or disappeared.".to_string())?;
            if !ItemIdentity::record(&before).same_item(&ItemIdentity::record(&now)) {
                return Err("Selected link target changed; select it again.".into());
            }
        }
        Ok(now)
    }
}
fn target_is_dir(target: &File, path: &Path) -> Result<bool, String> {
    target.metadata().map(|metadata| metadata.is_dir())
        .map_err(|error| format!("Could not inspect {}: {}.", path.display(), io_message(&error)))
}
// A path held for its own kind, with the link's target held beside it when it is one; the metadata is the first check's.
fn open_held(path: &Path, mut open: impl FnMut(&Path, bool) -> Result<Held, String>) -> Result<(Held, Metadata), String> {
    let mut held = open(path, false)?;
    let first = held.current()?;
    if first.file_type().is_symlink() {
        held.target = Some(open(path, true)?.file);
    }
    Ok((held, first))
}
// The kind of what a mark or save target points at, worded without the path prefix target_is_dir adds.
fn held_is_dir(held: &Held, metadata: &Metadata) -> Result<bool, String> {
    match &held.target {
        Some(target) => target.metadata().map(|target| target.is_dir()).map_err(|error| io_message(&error)),
        None => Ok(metadata.is_dir()),
    }
}
fn inspect_added_path(path: &Path, open: impl FnMut(&Path, bool) -> Result<Held, String>) -> Result<(Held, bool), String> {
    let (held, _) = open_held(path, open)?;
    let is_dir = match &held.target {
        Some(target) => target_is_dir(target, path)?,
        None => held.current()?.is_dir(),
    };
    Ok((held, is_dir))
}
struct SaveReview {
    id: usize,
    folder: Held,
    path: PathBuf,
    target: Option<Held>,
}
#[derive(Default)]
struct State {
    marks: Vec<Held>,
    save: Option<SaveReview>,
    folder: folder::Draft,
}
impl State {
    // Sample input: {"c":"picker","op":"mark","id":1,"path":"/tmp/photo.png","multiple":true}.
    fn handle(&mut self, line: &str, cancel: &Cancellation) -> Result<String, String> {
        cancel.check()?;
        let id = field_usize(line, "id").filter(|id| *id > 0).ok_or("Picker request has no identity.")?;
        match field_str(line, "op").as_deref() {
            Some("mark") => {
                let path = PathBuf::from(field_str(line, "path").unwrap_or_default());
                if let Some(index) = self.marks.iter().position(|item| item.path == path) {
                    self.marks.remove(index);
                } else {
                    let (held, _) = open_held(&path, Held::open)?;
                    let metadata = held.current()?;
                    let directory = held_is_dir(&held, &metadata)?;
                    if directory != field_bool(line, "directory") { return Err("This item is not the requested file type.".into()); }
                    if !field_bool(line, "multiple") { self.marks.clear(); }
                    self.marks.push(held);
                }
                self.valid_marks(cancel)
            }
            Some("select") => {
                let paths: Vec<PathBuf> = field_str_array(line, "paths").into_iter().map(PathBuf::from).collect();
                let directory = field_bool(line, "directory");
                self.select(&paths, directory, cancel, Held::open)
            }
            Some("validate") => self.valid_marks(cancel),
            Some("folderPrepare") => self.folder.prepare(line, id),
            Some("folderCreate") => self.folder.create(line, cancel),
            Some("folderClose") => self.folder.close(),
            Some("save") => {
                self.save = None;
                let path = PathBuf::from(field_str(line, "folder").unwrap_or_default());
                let name = field_str(line, "name").unwrap_or_default();
                if let Some(refusal) = super::ops::name_refusal_sentence(&name) { return Err(refusal); }
                let folder = Held::open(&path, true)?;
                if !folder.current()?.is_dir() { return Err("The selected save location is not a folder.".into()); }
                let path = path.join(name);
                let target = match path.symlink_metadata() {
                    Ok(_) => {
                        let (held, metadata) = open_held(&path, Held::open)?;
                        if held_is_dir(&held, &metadata)? { return Err("A folder has this name; choose another name.".into()); }
                        Some(held)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(format!("Could not inspect output location: {}.", io_message(&error))),
                };
                if let Some(target) = &target { target.current()?; }
                let result = format!(r#""review":{},"path":"{}","collision":{}"#, id, escape(&path.to_string_lossy()), target.is_some());
                self.save = Some(SaveReview { id, folder, path, target });
                Ok(result)
            }
            Some("review") => {
                let review = self.save.as_ref().filter(|review| Some(review.id) == field_usize(line, "review"))
                    .ok_or("The save location changed; review it again.")?;
                review.folder.current()?;
                if let Some(target) = &review.target {
                    target.current()?;
                } else {
                    match review.path.symlink_metadata() {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                        Ok(_) => return Err("An item with this name just appeared there; review it again.".into()),
                        Err(error) => return Err(format!("Could not inspect output location: {}.", io_message(&error))),
                    }
                }
                Ok(format!(r#""path":"{}","collision":{}"#, escape(&review.path.to_string_lossy()), review.target.is_some()))
            }
            _ => Err("Unknown picker check.".into()),
        }
    }
    fn select(&mut self, paths: &[PathBuf], directory: bool, cancel: &Cancellation,
        mut open: impl FnMut(&Path, bool) -> Result<Held, String>) -> Result<String, String> {
        if paths.iter().any(|path| !path.is_absolute()) { return Err("Picker paths must be absolute.".into()); }
        self.check_budget(paths, directory)?;
        let mut wanted: HashSet<_> = paths.iter().collect();
        for held in &self.marks {
            if !wanted.contains(&held.path) { continue; }
            if held.is_dir()? != directory { wanted.remove(&held.path); }
        }
        let mut seen: HashSet<_> = self.marks.iter().map(|held| held.path.clone()).collect();
        let mut added = Vec::new();
        let mut skipped = Vec::new();
        for path in paths {
            cancel.check()?;
            if !seen.insert(path.clone()) { continue; }
            let (held, is_dir) = match inspect_added_path(path, &mut open) {
                Ok(inspected) => inspected,
                Err(error) => {
                    let prefix = format!("Could not inspect {}: ", path.display());
                    // The picker joins this clause into its own sentence, so it carries neither the prefix nor the closing period.
                    let why = error.strip_prefix(&prefix).unwrap_or(&error).trim_end_matches('.');
                    skipped.push(format!(r#"{{"path":"{}","why":"{}"}}"#, escape(&path.to_string_lossy()), escape(why)));
                    continue;
                }
            };
            if is_dir == directory { added.push(held); }
        }
        cancel.check()?;
        self.marks.retain(|held| wanted.contains(&held.path));
        self.marks.extend(added);
        self.valid_marks(cancel).map(|marks| format!(r#"{},"skipped":[{}]"#, marks, skipped.join(",")))
    }
    fn check_budget(&self, paths: &[PathBuf], directory: bool) -> Result<(), String> {
        limits::raise_soft_to_hard();
        let soft = limits::soft_limit()?;
        let budget = soft.saturating_sub(limits::DESCRIPTOR_RESERVE);
        let mut descriptors: usize = self.marks.iter().map(|held| if held.target.is_some() { limits::SYMLINK_DESCRIPTORS } else { 1 }).sum();
        let mut seen: HashSet<_> = self.marks.iter().map(|held| &held.path).collect();
        for path in paths {
            if !seen.insert(path) { continue; }
            if let Ok(metadata) = path.symlink_metadata() {
                if metadata.file_type().is_symlink() {
                    descriptors = descriptors.saturating_add(limits::SYMLINK_DESCRIPTORS);
                } else if metadata.is_dir() == directory {
                    descriptors = descriptors.saturating_add(1);
                }
            }
        }
        if descriptors > budget {
            let count = paths.iter().collect::<HashSet<_>>().len();
            return Err(format!("Cannot select {} items. The file chooser takes at most {}.", count, budget));
        }
        Ok(())
    }
    fn valid_marks(&mut self, cancel: &Cancellation) -> Result<String, String> {
        let mut rows = Vec::new();
        let mut missing = 0;
        self.marks.retain(|item| {
            if cancel.check().is_err() { return false; }
            match item.current() {
                Ok(meta) => {
                    rows.push(format!(r#"{{"path":"{}","bytes":{}}}"#, escape(&item.path.to_string_lossy()), meta.len()));
                    true
                }
                Err(_) => { missing += 1; false }
            }
        });
        cancel.check()?;
        Ok(format!(r#""marks":[{}],"removed":{}"#, rows.join(","), missing))
    }
}
fn response(line: &str, result: Result<String, String>) -> String {
    let id = field_usize(line, "id").unwrap_or(0);
    let op = field_str(line, "op").unwrap_or_default();
    let body = match result {
        Ok(fields) => format!(r#""ok":true,{}"#, fields),
        Err(error) => format!(r#""ok":false,"error":"{}""#, escape(&error)),
    };
    format!(r#"{{"t":"picker","id":{},"op":"{}",{}}}"#, id, escape(&op), body)
}

fn matches(name: &str, globs: &[CString], mimes: &[String], db: &Db) -> bool {
    CString::new(name).is_ok_and(|name| globs.iter().any(|glob| unsafe { fnmatch(glob.as_ptr(), name.as_ptr(), FNM_CASEFOLD) == 0 }))
        || (!mimes.is_empty() && db.lookup(name).is_some_and(|mime| mimes.iter().any(|wanted| wanted == mime || wanted.strip_suffix("/*").is_some_and(|prefix| mime.starts_with(&format!("{}/", prefix))))))
}
pub fn filter_listing(listing: &mut Listing, db: &Db, line: &str) {
    let globs: Vec<_> = field_str_array(line, "pickerGlobs").into_iter().filter_map(|glob| CString::new(glob).ok()).collect();
    let mimes = field_str_array(line, "pickerMimes");
    if globs.is_empty() && mimes.is_empty() { return; }
    let names = &listing.names;
    // Keep every symlink: d_type identifies links without statting their targets outside the viewport.
    listing.spans.retain(|span| span.is_dir || span.is_symlink || matches(&names[span.off as usize..(span.off + span.len) as usize], &globs, &mimes, db));
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "picker_wording_tests.rs"]
mod wording;

#[cfg(test)]
#[path = "picker_folder_tests.rs"]
mod folder_tests;
