//! The directory a stand-in omdrop lives in for one test.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

// A stand-in omdrop's own directory: fresh for every call, so nothing an earlier run left is read, and removed with it.
pub(super) struct StandInDir(PathBuf);

impl StandInDir {
    pub(super) fn fresh(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("flea-omdrop-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
        Self(root)
    }
}

impl std::ops::Deref for StandInDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for StandInDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for StandInDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
