// Picker checks retain reviewed objects; only the application receiving a URI writes its contents.
use super::listing::Listing;
use super::mime::Db;
use super::opsreq::OpMsg;
use super::trashmanifest::Cancellation;
use super::undo::ItemIdentity;
use crate::error::io_message;
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize};
use std::ffi::CString;
use std::fs::{File, Metadata, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Sender, SyncSender, TrySendError};

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
                TrySendError::Full(line) => (line, "A picker check is still running; try again."),
                TrySendError::Disconnected(line) => (line, "The picker check stopped; reopen this request."),
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
            .open(path).map_err(|error| format!("Could not inspect {}: {}", path.display(), io_message(&error)))?;
        Ok(Self { path: path.into(), file, follow, target: None })
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
                    let mut held = Held::open(&path, false)?;
                    if held.current()?.file_type().is_symlink() {
                        held.target = Some(Held::open(&path, true)?.file);
                    }
                    let metadata = held.current()?;
                    let directory = match &held.target {
                        Some(target) => target.metadata().map_err(|error| io_message(&error))?.is_dir(),
                        None => metadata.is_dir(),
                    };
                    if directory != field_bool(line, "directory") { return Err("This item is not the requested file type.".into()); }
                    if !field_bool(line, "multiple") { self.marks.clear(); }
                    self.marks.push(held);
                }
                self.valid_marks(cancel)
            }
            Some("validate") => self.valid_marks(cancel),
            Some("save") => {
                self.save = None;
                let path = PathBuf::from(field_str(line, "folder").unwrap_or_default());
                let name = field_str(line, "name").unwrap_or_default();
                if !super::ops::valid_name(&name) { return Err("Use a filename without a separator.".into()); }
                let folder = Held::open(&path, true)?;
                if !folder.current()?.is_dir() { return Err("The selected save location is not a directory.".into()); }
                let path = path.join(name);
                let target = match path.symlink_metadata() {
                    Ok(_) => {
                        let mut held = Held::open(&path, false)?;
                        let metadata = held.current()?;
                        if metadata.file_type().is_symlink() {
                            held.target = Some(Held::open(&path, true)?.file);
                        }
                        let directory = match &held.target {
                            Some(target) => target.metadata().map_err(|error| io_message(&error))?.is_dir(),
                            None => metadata.is_dir(),
                        };
                        if directory { return Err("This output name is a directory; choose a filename.".into()); }
                        Some(held)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(format!("Could not inspect output location: {}", io_message(&error))),
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
                        Ok(_) => return Err("An item appeared at this output location; review it again.".into()),
                        Err(error) => return Err(format!("Could not inspect output location: {}", error)),
                    }
                }
                Ok(format!(r#""path":"{}","collision":{}"#, escape(&review.path.to_string_lossy()), review.target.is_some()))
            }
            _ => Err("Unknown picker check.".into()),
        }
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
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn inaccessible_selection_reports_plain_cause_and_recovers() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("picker-plain-error");
        let parent = dir.dir("locked");
        let path = dir.file("locked/item", "retained contents");
        let mut state = State::default();
        let cancel = Cancellation::default();
        let mark = format!(r#"{{"op":"mark","id":1,"path":"{}"}}"#, escape(&path.to_string_lossy()));
        dir.assert_contains(&parent);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0)).unwrap();
        let refused = state.handle(&mark, &cancel);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(refused.unwrap_err(), format!("Could not inspect {}: permission denied", path.display()));
        assert!(state.marks.is_empty());
        state.handle(&mark, &cancel).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"retained contents");
    }

    #[test]
    fn marks_keep_order_across_navigation_and_drop_missing_or_replaced_items() {
        let dir = TestDir::new("picker-marks");
        let first = dir.file("first", "one");
        let second = dir.file("second", "two");
        let mut state = State::default();
        let cancel = Cancellation::default();
        for path in [&first, &second] {
            let line = format!(r#"{{"op":"mark","id":1,"path":"{}","multiple":true}}"#, escape(&path.to_string_lossy()));
            assert!(state.handle(&line, &cancel).unwrap().contains(r#""removed":0"#));
        }
        std::fs::rename(&first, dir.join("retained")).unwrap();
        dir.file("first", "replacement");
        let reply = state.handle(r#"{"op":"validate","id":2}"#, &cancel).unwrap();
        assert!(reply.contains(r#""removed":1"#));
        assert_eq!(state.marks.len(), 1);
        assert_eq!(state.marks[0].path, second);
        std::fs::rename(&second, dir.join("second-retained")).unwrap();
        assert!(state.handle(r#"{"op":"validate","id":3}"#, &cancel).unwrap().contains(r#""marks":[]"#));
    }

    #[test]
    fn save_review_detects_arrivals_and_replacements_without_writing() {
        let dir = TestDir::new("picker-save");
        let mut state = State::default();
        let cancel = Cancellation::default();
        let probe = format!(r#"{{"op":"save","id":4,"folder":"{}","name":"result.txt"}}"#, escape(&dir.path().to_string_lossy()));
        assert!(state.handle(&probe, &cancel).unwrap().contains(r#""collision":false"#));
        assert!(!dir.join("result.txt").exists());
        let path = dir.file("result.txt", "caller data");
        assert!(state.handle(r#"{"op":"review","id":5,"review":4}"#, &cancel).is_err());
        assert!(state.handle(&probe, &cancel).unwrap().contains(r#""collision":true"#));
        assert!(state.handle(r#"{"op":"review","id":6,"review":4}"#, &cancel).is_ok());
        std::fs::rename(&path, dir.join("retained")).unwrap();
        dir.file("result.txt", "replacement");
        assert!(state.handle(r#"{"op":"review","id":7,"review":4}"#, &cancel).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
    }

    #[test]
    fn filters_keep_directories_and_matches_beyond_the_first_window() {
        let mut listing = Listing::new();
        listing.push("folder", true);
        for index in 0..150 { listing.push(&format!("file-{}.txt", index), false); }
        listing.push("last.PNG", false);
        filter_listing(&mut listing, &Db::from_str(""), r#"{"pickerGlobs":["*.png"]}"#);
        assert_eq!(listing.len(), 2);
        assert_eq!(listing.name(1), "last.PNG");
    }

    #[test]
    fn filters_keep_symlink_navigation_without_inspecting_targets() {
        let dir = TestDir::new("picker-filter-links");
        let folder = dir.dir("folder");
        let link = dir.join("linked-folder");
        let broken = dir.join("broken-link");
        std::os::unix::fs::symlink(&folder, &link).unwrap();
        std::os::unix::fs::symlink(dir.join("missing-target"), &broken).unwrap();
        dir.file("hidden-by-filter.txt", "text");
        let (mut listing, _) = super::super::scan::scan(&dir.path().to_string_lossy(), false).unwrap();
        filter_listing(&mut listing, &Db::from_str(""), r#"{"pickerGlobs":["*.png"]}"#);
        let names: Vec<_> = (0..listing.len()).map(|index| listing.name(index)).collect();
        assert!(names.contains(&"folder"));
        assert!(names.contains(&"linked-folder"));
        assert!(names.contains(&"broken-link"));
        assert!(!names.contains(&"hidden-by-filter.txt"));
        let (mut recent, _) = super::super::listpaths::listing_of(&[link.to_string_lossy().into(), broken.to_string_lossy().into()]);
        filter_listing(&mut recent, &Db::from_str(""), r#"{"pickerMimes":["image/png"]}"#);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn save_refuses_directories_and_links_to_directories() {
        let dir = TestDir::new("picker-save-directory");
        let folder = dir.dir("folder");
        std::os::unix::fs::symlink(folder, dir.join("linked-folder")).unwrap();
        let mut state = State::default();
        for name in ["folder", "linked-folder"] {
            let line = format!(r#"{{"op":"save","id":1,"folder":"{}","name":"{}"}}"#, escape(&dir.path().to_string_lossy()), name);
            let error = state.handle(&line, &Cancellation::default()).unwrap_err();
            assert!(error.contains("is a directory"));
            assert!(state.save.is_none());
        }
    }

    #[test]
    fn directory_symlink_keeps_its_uri_and_rejects_a_replaced_target() {
        let dir = TestDir::new("picker-link");
        let target = dir.dir("target");
        let link = dir.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut state = State::default();
        let cancel = Cancellation::default();
        let line = format!(r#"{{"op":"mark","id":1,"path":"{}","directory":true}}"#, escape(&link.to_string_lossy()));
        assert!(state.handle(&line, &cancel).is_ok());
        assert_eq!(state.marks[0].path, link);
        assert_eq!(state.marks[0].current().unwrap().len(), link.symlink_metadata().unwrap().len());
        std::fs::rename(&target, dir.join("retained-target")).unwrap();
        dir.dir("target");
        assert!(state.handle(r#"{"op":"validate","id":2}"#, &cancel).unwrap().contains(r#""removed":1"#));
    }

    #[test]
    fn single_selection_replaces_toggles_and_refuses_wrong_types() {
        let dir = TestDir::new("picker-single");
        let first = dir.file("first", "one");
        let second = dir.file("second", "two");
        let mut state = State::default();
        let cancel = Cancellation::default();
        let mark = |path: &Path| format!(r#"{{"op":"mark","id":1,"path":"{}"}}"#, escape(&path.to_string_lossy()));
        state.handle(&mark(&first), &cancel).unwrap();
        state.handle(&mark(&second), &cancel).unwrap();
        assert_eq!(state.marks.len(), 1);
        assert_eq!(state.marks[0].path, second);
        assert!(state.handle(&mark(dir.path()), &cancel).is_err());
        assert_eq!(state.marks[0].path, second);
        state.handle(&mark(&second), &cancel).unwrap();
        assert!(state.marks.is_empty());
        assert!(state.handle(&mark(Path::new("relative")), &cancel).is_err());
    }

    #[test]
    fn save_review_rejects_parent_replacement_bad_names_and_cancellation() {
        let dir = TestDir::new("picker-parent");
        let folder = dir.dir("folder");
        let mut state = State::default();
        let cancel = Cancellation::default();
        let probe = |name: &str| format!(r#"{{"op":"save","id":1,"folder":"{}","name":"{}"}}"#, escape(&folder.to_string_lossy()), escape(name));
        for name in ["", ".", "..", "../outside", "nul\0name"] {
            assert!(state.handle(&probe(name), &cancel).is_err());
        }
        state.handle(&probe("output"), &cancel).unwrap();
        std::fs::rename(&folder, dir.join("retained-folder")).unwrap();
        dir.dir("folder");
        assert!(state.handle(r#"{"op":"review","id":2,"review":1}"#, &cancel).is_err());
        cancel.next();
        assert!(state.handle(&probe("output"), &cancel).is_err());
        assert!(!folder.join("output").exists());
    }

    #[test]
    fn mime_and_glob_rules_are_alternatives_and_all_files_restores_rows() {
        let db = Db::from_str("50:text/plain:*.txt\n50:image/png:*.png\n");
        let mut listing = Listing::new();
        for name in ["notes.txt", "photo.png", "archive.zip"] { listing.push(name, false); }
        filter_listing(&mut listing, &db, r#"{"pickerGlobs":["*.zip"],"pickerMimes":["image/*"]}"#);
        assert_eq!(listing.len(), 2);
        assert_eq!(listing.name(0), "photo.png");
        let mut all = Listing::new();
        all.push("notes.txt", false);
        filter_listing(&mut all, &db, r#"{"pickerGlobs":[],"pickerMimes":[]}"#);
        assert_eq!(all.len(), 1);
    }
}
