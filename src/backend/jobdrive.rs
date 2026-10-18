use std::path::Path;

// st_dev keys ordinary filesystems; btrfs subvolumes use the mount's source because their anonymous devices differ.
pub(super) fn drive_of(
    target: &Path,
    body: &str,
) -> Result<(String, String), crate::error::FleaError> {
    let owned = target.to_path_buf();
    let mounts = body.to_string();
    probe(target, body, super::iomount::CALL_DEADLINE, move || {
        resolve(&owned, &mounts)
    })
}

// A target on a mount that can wedge resolves on the mount's worker, so a dead server fails the preparation.
fn probe(
    target: &Path,
    body: &str,
    deadline: std::time::Duration,
    work: impl FnOnce() -> (String, String) + Send + 'static,
) -> Result<(String, String), crate::error::FleaError> {
    super::iomount::call_with(
        target,
        body,
        "jobs",
        deadline,
        super::iomount::STUCK_TTL,
        work,
    )
}

fn resolve(target: &Path, body: &str) -> (String, String) {
    use std::os::unix::fs::MetadataExt;
    let mut path = target.to_path_buf();
    while path.symlink_metadata().is_err() && path.pop() {}
    let path = path.canonicalize().unwrap_or(path);
    let entry = super::mountinfo::mount_entry_in(&path, body);
    let key = if let Some(entry) = entry.as_ref().filter(|e| e.fstype == "btrfs") {
        format!("btrfs:{}", entry.source)
    } else {
        path.metadata()
            .map(|m| format!("dev:{}", m.dev()))
            .unwrap_or_else(|_| {
                format!("mount:{}", super::iomount::mount_key(&path, body).display())
            })
    };
    let mount_name = entry
        .as_ref()
        .filter(|e| e.mount != Path::new("/") && e.mount != Path::new("/home"))
        .and_then(|e| e.mount.file_name());
    let name = mount_name
        .or_else(|| target.file_name())
        .unwrap_or_default()
        .to_string_lossy()
        .into();
    (key, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::time::Duration;

    #[test]
    fn stalled_target_resolution_uses_the_mount_deadline() {
        let root = TestDir::new("job-drive-deadline");
        let body = format!(
            "1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:45 / {} rw - nfs host:/share rw\n",
            root.path().display()
        );
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = std::sync::mpsc::channel();
        let result = probe(root.path(), &body, Duration::from_millis(20), move || {
            let _ = wait.recv();
            finished.send(()).unwrap();
            ("drive".into(), "target".into())
        });
        let error = result.unwrap_err();
        assert!(error.msg.contains("is not responding"));
        assert!(error.msg.contains(root.path().to_str().unwrap()));
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
