use flea::bytename::ExactText as _;
use super::{terminal::Terminal, wire::{number, word, Wire}};
use crate::backend::{ops::valid_name, regfile};
use crate::oflags::O_NOFOLLOW;
use std::collections::{BTreeSet, VecDeque};
use std::fs::{self, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const NAME_BYTES: usize = 255;
// The one sentence for every fault in making the temporary folder the names file lives in; a fault after that names the folder.
const PREPARE_FAILED: &str = "tui.batch.prepare_failed";

pub struct Batch {
    sources: Vec<(PathBuf, (u64, u64, u32))>,
    pending: VecDeque<(PathBuf, String)>,
    pub completed: usize,
    pub total: usize,
    pub last: Option<PathBuf>,
    pub cancelled: bool,
}
fn identity(meta: &Metadata) -> (u64, u64, u32) { (meta.dev(), meta.ino(), meta.mode() & 0o170000) }
fn failure(key: &'static str) -> io::Error { io::Error::other(super::words::Text::Key(key)) }
fn semantic(error: &io::Error) -> super::words::Text {
    error.get_ref().and_then(|e| e.downcast_ref::<super::words::Text>()).cloned()
        .unwrap_or_else(|| crate::error::io_message(error).into())
}
fn fault(key: &'static str, args: Vec<super::words::Text>) -> io::Error { io::Error::other(super::words::Text::Args(key, args)) }
impl Batch {
    pub fn new(paths: Vec<PathBuf>) -> io::Result<Self> {
        let mut sources = Vec::with_capacity(paths.len());
        let mut unique = BTreeSet::new();
        for path in paths {
            if !path.is_absolute() || !unique.insert(path.clone()) { return Err(failure("tui.batch.not_unique")); }
            let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| failure("tui.batch.invalid_text"))?;
            if name.contains(['\n', '\r']) { return Err(failure("tui.batch.line_break")); }
            let meta = path.symlink_metadata()?;
            sources.push((path, identity(&meta)));
        }
        Ok(Self { sources, pending: VecDeque::new(), completed: 0, total: 0, last: None, cancelled: false })
    }
    pub fn edit(&mut self, terminal: &mut Terminal) -> io::Result<()> {
        let editor = super::external_editor::chosen().ok_or_else(|| failure("tui.batch.no_editor"))?;
        let document = self.sources.iter().map(|(path, _)| path.file_name().unwrap().to_str().unwrap()).collect::<Vec<_>>().join("\n") + "\n";
        let mut names = NamesFile::new(&document)?;
        let result = terminal.run_editor(&editor, &names.path).and_then(|_| {
            names.check()?;
            let text = read_names(&names.path, self.sources.len())?;
            self.validate(&text)
        });
        settle(result, names.cleanup())
    }
    fn validate(&mut self, document: &str) -> io::Result<()> {
        self.pending.clear();
        self.total = 0;
        let lines: Vec<&str> = document.strip_suffix('\n').unwrap_or(document).split('\n').collect();
        if lines.len() != self.sources.len() { return Err(failure("tui.batch.count_changed")); }
        let mut destinations = BTreeSet::new();
        let mut pending = VecDeque::new();
        let mut selected: Vec<&Path> = self.sources.iter().map(|(path, _)| path.as_path()).collect();
        selected.sort_unstable();
        for ((path, before), name) in self.sources.iter().zip(lines) {
            if !valid_name(name) || name.contains('\r') || name.len() > NAME_BYTES { return Err(failure("tui.batch.invalid_name")); }
            if identity(&path.symlink_metadata()?) != *before { return Err(failure("tui.batch.source_changed")); }
            let destination = path.parent().ok_or_else(|| failure("tui.batch.no_parent"))?.join(name);
            if !destinations.insert(destination.clone()) { return Err(failure("tui.batch.collision")); }
            if destination == *path { continue; }
            match destination.symlink_metadata() {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Ok(_) => return Err(failure("tui.batch.exists")),
                Err(error) => return Err(error),
            }
            let position = selected.binary_search(&path.as_path()).unwrap();
            if before.2 == 0o040000 && selected.get(position + 1).is_some_and(|other| other.starts_with(path)) {
                return Err(failure("tui.batch.nested"));
            }
            pending.push_back((path.clone(), name.into()));
        }
        self.total = pending.len();
        self.pending = pending;
        Ok(())
    }
    pub fn send_next(&self, wire: &mut Wire, id: usize) -> io::Result<bool> {
        let Some((path, name)) = self.pending.front().filter(|_| !self.cancelled) else {
            return Ok(false);
        };
        wire.send(vec![
            ("c", word("rename")),
            ("path", word(&path.exact_text())),
            ("to", word(name)),
            ("menuId", number(id)),
        ])?;
        Ok(true)
    }
    pub fn complete(&mut self, path: &str) -> io::Result<()> {
        let Some((source, name)) = self.pending.front() else { return Err(failure("tui.batch.unexpected")); };
        let expected = source.parent().unwrap().join(name);
        if expected != Path::new(path) { return Err(failure("tui.batch.mismatched")); }
        self.pending.pop_front();
        self.completed += 1;
        self.last = Some(expected);
        Ok(())
    }
}

// The edited names read back, refused past what the selection could name: Linux filename components hold at most 255 bytes, plus one newline per name.
fn read_names(path: &Path, count: usize) -> io::Result<String> {
    let limit = count.checked_mul(NAME_BYTES + 1).ok_or_else(|| failure("tui.batch.too_many"))? as u64;
    let mut file = regfile::open_if_regular(path, O_NOFOLLOW)?.take(limit + 1);
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    if text.len() as u64 > limit { return Err(failure("tui.batch.too_long")); }
    Ok(text)
}
// A cleanup fault joins an earlier error, and otherwise becomes the result.
fn settle(result: io::Result<()>, cleanup: io::Result<()>) -> io::Result<()> {
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(fault("tui.batch.cleanup_failed", vec![semantic(&error), semantic(&cleanup)])),
        (result, cleanup) => result.and(cleanup),
    }
}

struct NamesFile { root: PathBuf, path: PathBuf, identity: (u64, u64, u32), cleaned: bool }
impl NamesFile {
    fn new(document: &str) -> io::Result<Self> {
        let output = crate::backend::locale::command("mktemp").args(["-d", "/tmp/flea-rename-XXXXXX"]).output()?;
        if !output.status.success() { return Err(failure(PREPARE_FAILED)); }
        let root = PathBuf::from(String::from_utf8(output.stdout).map_err(|_| failure(PREPARE_FAILED))?.trim());
        if !root.is_absolute() || root.parent() != Some(Path::new("/tmp")) || !root.file_name().is_some_and(|name| name.to_string_lossy().starts_with("flea-rename-")) {
            return Err(failure(PREPARE_FAILED));
        }
        let meta = root.symlink_metadata()?;
        if !meta.is_dir() { return Err(failure(PREPARE_FAILED)); }
        let owned = identity(&meta);
        let names = Self { path: root.join("names"), root, identity: owned, cleaned: false };
        OpenOptions::new().write(true).create_new(true).mode(0o600).open(&names.path)?.write_all(document.as_bytes())?;
        Ok(names)
    }
    fn check(&self) -> io::Result<()> {
        if !self.root.is_absolute() || self.root.as_os_str().is_empty() || identity(&self.root.symlink_metadata()?) != self.identity {
            return Err(fault("tui.batch.folder_changed", vec![self.root.to_string_lossy().into_owned().into()]));
        }
        Ok(())
    }
    fn cleanup(&mut self) -> io::Result<()> {
        self.cleaned = true;
        self.check()?;
        if !self.path.is_absolute() || self.path == self.root || !self.path.starts_with(&self.root) {
            return Err(fault("tui.batch.cleanup_outside", vec![self.path.to_string_lossy().into_owned().into(), self.root.to_string_lossy().into_owned().into()]));
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::remove_dir(&self.root).map_err(|error| fault("tui.batch.cleanup_left", vec![self.root.to_string_lossy().into_owned().into(), semantic(&error)]))
    }
}
impl Drop for NamesFile {
    fn drop(&mut self) {
        if !self.cleaned { if let Err(error) = self.cleanup() { eprintln!("flea: {}", error); } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    fn rendered(error: io::Error) -> String { semantic(&error).render(&crate::tui::render::cards::words_tests::english()) }
    #[test]
    fn bulk_input_is_validated_whole_before_any_rename() {
        let directory = TestDir::new("tui-bulk");
        let a = directory.file("a", "one");
        let b = directory.file("b", "two");
        let mut batch = Batch::new(vec![a.clone(), b.clone()]).unwrap();
        for names in ["one\n", "same\nsame\n", "../outside\ntwo\n", "b\na\n", "\ntwo\n"] {
            assert!(batch.validate(names).is_err());
            assert_eq!(fs::read_to_string(&a).unwrap(), "one");
            assert_eq!(fs::read_to_string(&b).unwrap(), "two");
            assert!(batch.pending.is_empty());
        }
        batch.validate("renamed-a\nrenamed-b\n").unwrap();
        assert_eq!(batch.total, 2);
        assert!(batch.complete(&directory.join("wrong").to_string_lossy()).is_err());
        batch.complete(&directory.join("renamed-a").to_string_lossy()).unwrap();
        assert_eq!(batch.completed, 1);
        assert_eq!(batch.pending.front().unwrap().0, b);
    }
    #[test]
    fn bulk_rename_refusals_use_plain_words() {
        use std::os::unix::ffi::OsStringExt;
        let unreadable = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
        assert_eq!(rendered(Batch::new(vec![unreadable]).err().unwrap()), "Bulk rename cannot handle a name that is not valid text");
        let directory = TestDir::new("tui-bulk-words");
        let names = directory.file("names", &"x".repeat(NAME_BYTES + 2));
        assert_eq!(rendered(read_names(&names, 1).unwrap_err()), "An edited name is too long");
        assert_eq!(read_names(&names, 2).unwrap().len(), NAME_BYTES + 2, "two names leave room for it");
        let joined = settle(Err(failure("first")), Err(failure("second"))).unwrap_err();
        assert_eq!(rendered(joined), "first; cleanup could not finish: second");
        assert_eq!(rendered(settle(Ok(()), Err(failure("second"))).unwrap_err()), "second");
    }
    // A temporary folder that changed, a cleanup aimed outside it and one that cannot empty it: each refuses without deleting a thing, in its own sentence.
    #[test]
    fn temporary_folder_faults_name_the_folder_and_delete_nothing() {
        let directory = TestDir::new("tui-bulk-folder");
        let root = directory.dir("work");
        let keep = directory.file("work/keep", "kept");
        let real = identity(&root.symlink_metadata().unwrap());
        let shown = root.display();
        let mut names = NamesFile { path: root.join("names"), root: root.clone(), identity: (0, 0, 0), cleaned: true };
        assert_eq!(rendered(names.check().unwrap_err()), format!("Bulk rename stopped because its temporary folder {shown} changed"));
        // The identity guard fires first on a changed folder, so cleanup of it says the folder changed, and deletes nothing.
        names.cleaned = false;
        assert_eq!(rendered(names.cleanup().unwrap_err()), format!("Bulk rename stopped because its temporary folder {shown} changed"));
        names = NamesFile { path: root.clone(), root: root.clone(), identity: real, cleaned: false };
        assert_eq!(rendered(names.cleanup().unwrap_err()), format!("Bulk rename would not remove {shown}, which is not inside its temporary folder {shown}"));
        names = NamesFile { path: root.join("names"), root: root.clone(), identity: real, cleaned: false };
        let left = rendered(names.cleanup().unwrap_err());
        assert!(left.starts_with(&format!("Bulk rename left temporary files at {}: ", root.display())), "{left}");
        assert_eq!(fs::read_to_string(keep).unwrap(), "kept");
    }
    #[test]
    fn editor_cannot_redirect_a_replaced_source() {
        let directory = TestDir::new("tui-bulk-identity");
        let path = directory.file("source", "original");
        let mut batch = Batch::new(vec![path.clone()]).unwrap();
        fs::rename(&path, directory.join("held-original")).unwrap();
        directory.file("source", "replacement");
        assert!(batch.validate("destination\n").is_err());
        assert!(batch.pending.is_empty());
        assert_eq!(fs::read_to_string(path).unwrap(), "replacement");
    }
}
