// A request's own output folder under the preview root, removed when the request ends.
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

pub(super) struct Work(pub(super) PathBuf);

impl Work {
    pub(super) fn new(root: &Path) -> Result<Self, String> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder
            .create(root.join("shown"))
            .map_err(|e| e.to_string())?;
        let n = super::NEXT.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("work-{}-{n}", std::process::id()));
        builder.recursive(false);
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        // Only this request's own folder under the preview root, never an empty or relative path.
        if self.0.is_absolute()
            && self
                .0
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("work-"))
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
