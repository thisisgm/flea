use super::{terminal::Terminal, wire::{number, word, Wire}};
use crate::backend::{ops::valid_name, regfile};
use crate::oflags::O_NOFOLLOW;
use std::collections::{BTreeSet, VecDeque};
use std::fs::{self, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const NAME_BYTES: usize = 255;

pub struct Batch {
    sources: Vec<(PathBuf, (u64, u64, u32))>,
    pending: VecDeque<(PathBuf, String)>,
    pub completed: usize,
    pub total: usize,
    pub last: Option<PathBuf>,
    pub cancelled: bool,
}
fn identity(meta: &Metadata) -> (u64, u64, u32) { (meta.dev(), meta.ino(), meta.mode() & 0o170000) }
fn failure(message: &str) -> io::Error { io::Error::other(message) }
impl Batch {
    pub fn new(paths: Vec<PathBuf>) -> io::Result<Self> {
        let mut sources = Vec::with_capacity(paths.len());
        let mut unique = BTreeSet::new();
        for path in paths {
            if !path.is_absolute() || !unique.insert(path.clone()) { return Err(failure("Rename selection is not a unique absolute selection")); }
            let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| failure("Rename requires UTF-8 filenames"))?;
            if name.contains(['\n', '\r']) { return Err(failure("Bulk rename cannot represent a filename containing a line break")); }
            let meta = path.symlink_metadata()?;
            sources.push((path, identity(&meta)));
        }
        Ok(Self { sources, pending: VecDeque::new(), completed: 0, total: 0, last: None, cancelled: false })
    }
    pub fn edit(&mut self, terminal: &mut Terminal) -> io::Result<()> {
        let editor = std::env::var("VISUAL").ok().filter(|value| !value.trim().is_empty())
            .or_else(|| std::env::var("EDITOR").ok().filter(|value| !value.trim().is_empty()))
            .ok_or_else(|| failure("No $EDITOR is set, so there is nothing to hand the names to"))?;
        let document = self.sources.iter().map(|(path, _)| path.file_name().unwrap().to_str().unwrap()).collect::<Vec<_>>().join("\n") + "\n";
        let mut names = NamesFile::new(&document)?;
        let result = terminal.run_editor(&editor, &names.path).and_then(|_| {
            names.check()?;
            // Linux filename components hold at most 255 bytes, plus one newline per selected name.
            let limit = self.sources.len().checked_mul(NAME_BYTES + 1).ok_or_else(|| failure("Rename selection is too large"))? as u64;
            let mut file = regfile::open_if_regular(&names.path, O_NOFOLLOW)?.take(limit + 1);
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            if text.len() as u64 > limit { return Err(failure("Edited names exceed the selected files' filename limits")); }
            self.validate(&text)
        });
        let cleanup = names.cleanup();
        match (result, cleanup) {
            (Err(error), Err(cleanup)) => Err(failure(&format!("{}; cleanup failed: {}", error, cleanup))),
            (result, cleanup) => result.and(cleanup),
        }
    }
    fn validate(&mut self, document: &str) -> io::Result<()> {
        self.pending.clear();
        self.total = 0;
        let lines: Vec<&str> = document.strip_suffix('\n').unwrap_or(document).split('\n').collect();
        if lines.len() != self.sources.len() { return Err(failure("Bulk rename refused: the number of names changed")); }
        let mut destinations = BTreeSet::new();
        let mut pending = VecDeque::new();
        let mut selected: Vec<&Path> = self.sources.iter().map(|(path, _)| path.as_path()).collect();
        selected.sort_unstable();
        for ((path, before), name) in self.sources.iter().zip(lines) {
            if !valid_name(name) || name.contains('\r') || name.len() > NAME_BYTES { return Err(failure("Bulk rename refused: a name is invalid")); }
            if identity(&path.symlink_metadata()?) != *before { return Err(failure("Bulk rename refused: a selected item changed")); }
            let destination = path.parent().ok_or_else(|| failure("Selected item has no parent"))?.join(name);
            if !destinations.insert(destination.clone()) { return Err(failure("Bulk rename refused: two names collide")); }
            if destination == *path { continue; }
            match destination.symlink_metadata() {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Ok(_) => return Err(failure("Bulk rename refused: a destination already exists")),
                Err(error) => return Err(error),
            }
            let position = selected.binary_search(&path.as_path()).unwrap();
            if before.2 == 0o040000 && selected.get(position + 1).is_some_and(|other| other.starts_with(path)) {
                return Err(failure("Rename selected folders separately from their selected contents"));
            }
            pending.push_back((path.clone(), name.into()));
        }
        self.total = pending.len();
        self.pending = pending;
        Ok(())
    }
    pub fn send_next(&self, wire: &mut Wire, id: usize) -> io::Result<bool> {
        let Some((path, name)) = self.pending.front().filter(|_| !self.cancelled) else { return Ok(false); };
        wire.send(vec![("c", word("rename")), ("path", word(&path.to_string_lossy())), ("to", word(name)), ("menuId", number(id))])?;
        Ok(true)
    }
    pub fn complete(&mut self, path: &str) -> io::Result<()> {
        let Some((source, name)) = self.pending.front() else { return Err(failure("Unexpected rename completion")); };
        let expected = source.parent().unwrap().join(name);
        if expected != Path::new(path) { return Err(failure("Rename completion did not match the pending item")); }
        self.pending.pop_front();
        self.completed += 1;
        self.last = Some(expected);
        Ok(())
    }
}

struct NamesFile { root: PathBuf, path: PathBuf, identity: (u64, u64, u32), cleaned: bool }
impl NamesFile {
    fn new(document: &str) -> io::Result<Self> {
        let output = Command::new("mktemp").args(["-d", "/tmp/flea-rename-XXXXXX"]).output()?;
        if !output.status.success() { return Err(failure("Could not create bulk rename directory")); }
        let root = PathBuf::from(String::from_utf8(output.stdout).map_err(|_| failure("Bulk rename temporary path is not UTF-8"))?.trim());
        if !root.is_absolute() || root.parent() != Some(Path::new("/tmp")) || !root.file_name().is_some_and(|name| name.to_string_lossy().starts_with("flea-rename-")) {
            return Err(failure("Bulk rename temporary path is outside its owned sandbox"));
        }
        let meta = root.symlink_metadata()?;
        if !meta.is_dir() { return Err(failure("Bulk rename temporary root is not a directory")); }
        let owned = identity(&meta);
        let names = Self { path: root.join("names"), root, identity: owned, cleaned: false };
        OpenOptions::new().write(true).create_new(true).mode(0o600).open(&names.path)?.write_all(document.as_bytes())?;
        Ok(names)
    }
    fn check(&self) -> io::Result<()> {
        if !self.root.is_absolute() || self.root.as_os_str().is_empty() || identity(&self.root.symlink_metadata()?) != self.identity {
            return Err(failure("Bulk rename temporary directory changed"));
        }
        Ok(())
    }
    fn cleanup(&mut self) -> io::Result<()> {
        self.cleaned = true;
        self.check()?;
        if !self.path.is_absolute() || self.path == self.root || !self.path.starts_with(&self.root) {
            return Err(failure("Refused cleanup outside the bulk rename sandbox"));
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::remove_dir(&self.root).map_err(|error| failure(&format!("Bulk rename scratch files remain at {}: {}", self.root.display(), error)))
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
