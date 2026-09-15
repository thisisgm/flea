// IN_ATTRIB includes chmod to the mode a file already has. Remember only attributes the listing
// can display, so repeated no-op chmods do not restart its directory-size work indefinitely.
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const LIMIT: usize = 256;

#[derive(PartialEq)]
struct Attributes {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    links: u64,
    size: u64,
    modified: (i64, i64),
}

impl Attributes {
    fn read(path: &Path) -> std::io::Result<Self> {
        let m = fs::symlink_metadata(path)?;
        Ok(Self { dev: m.dev(), ino: m.ino(), mode: m.mode(), uid: m.uid(), gid: m.gid(),
                  links: m.nlink(), size: m.size(), modified: (m.mtime(), m.mtime_nsec()) })
    }
}

#[derive(Default)]
pub struct AttributeCache {
    values: HashMap<OsString, Attributes>,
    order: VecDeque<OsString>,
}

impl AttributeCache {
    // First observations and unreadable entries refresh conservatively; no directory sweep.
    pub fn changed(&mut self, directory: &Path, name: &std::ffi::OsStr) -> bool {
        let Ok(next) = Attributes::read(&directory.join(name)) else {
            self.values.remove(name);
            self.order.retain(|item| item != name);
            return true;
        };
        let changed = self.values.get(name) != Some(&next);
        if !self.values.contains_key(name) {
            if self.order.len() == LIMIT {
                if let Some(oldest) = self.order.pop_front() { self.values.remove(&oldest); }
            }
            self.order.push_back(name.to_owned());
        }
        self.values.insert(name.to_owned(), next);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::ffi::OsStr;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn repeated_chmod_is_quiet_but_real_mode_and_size_changes_refresh() {
        let d = TestDir::new("watch-attributes");
        let file = d.file("entry", "abc");
        let name = OsStr::new("entry");
        let mut cache = AttributeCache::default();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(cache.changed(d.path(), name));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!cache.changed(d.path(), name));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(cache.changed(d.path(), name));
        fs::write(&file, "longer").unwrap();
        assert!(cache.changed(d.path(), name));
        fs::remove_file(&file).unwrap();
        assert!(cache.changed(d.path(), name));
    }

    #[test]
    fn symlinks_are_not_followed_and_replacements_are_not_suppressed() {
        let d = TestDir::new("watch-attribute-symlink");
        let target = d.file("target", "abc");
        symlink(&target, d.join("link")).unwrap();
        let name = OsStr::new("link");
        let mut cache = AttributeCache::default();
        assert!(cache.changed(d.path(), name));
        fs::write(target, "longer").unwrap();
        assert!(!cache.changed(d.path(), name));
        fs::rename(d.join("link"), d.join("old-link")).unwrap();
        symlink(d.join("target"), d.join("link")).unwrap();
        assert!(cache.changed(d.path(), name));
    }

    #[test]
    fn remembered_attributes_are_bounded() {
        let d = TestDir::new("watch-attribute-limit");
        let mut cache = AttributeCache::default();
        for i in 0..=LIMIT {
            let name = format!("entry-{i}");
            d.file(&name, "");
            assert!(cache.changed(d.path(), OsStr::new(&name)));
        }
        assert_eq!(cache.values.len(), LIMIT);
        assert_eq!(cache.order.len(), LIMIT);
        assert!(!cache.values.contains_key(OsStr::new("entry-0")));
    }
}
