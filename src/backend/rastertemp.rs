//! The preview raster helper's temp names and output bound, shared by the helper and the GUI that runs it.
use std::path::{Path, PathBuf};

/// The largest PNG the helper hands back; its parent reads the helper's output under the same bound.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// Every temp a process makes starts with its pid, so a sweep can tell one process's temps from another's.
pub fn temp_prefix_of(pid: u32) -> String {
    format!(".flea-{pid}-")
}

/// Where the helper writes its transient rasters, apart from the shared thumbnail cache.
pub fn preview_dir() -> PathBuf {
    crate::userfile::cache_home().join("flea").join("preview")
}

/// A helper killed mid-thumbnail never runs its own cleanup, so its parent removes the temps in `dir` named by its pid.
pub fn discard_in(dir: &Path, pid: u32) {
    if !dir.is_absolute() {
        return;
    }
    let prefix = temp_prefix_of(pid);
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let name = item.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".png") {
            let _ = std::fs::remove_file(item.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_quality_temps_are_discarded_by_pid() {
        let dir = crate::backend::testdir::TestDir::new("rastertemp-discard");
        let names = [
            ".flea-4242-aa.png",
            ".flea-42421-bb.png",
            ".flea-4243-cc.png",
            ".flea-4242-dd.tmp",
            "kept.png",
        ];
        for name in names {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        discard_in(dir.path(), 4242);
        let mut left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|item| item.file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".flea-test-sandbox")
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                ".flea-4242-dd.tmp",
                ".flea-42421-bb.png",
                ".flea-4243-cc.png",
                "kept.png"
            ]
        );
    }
}
