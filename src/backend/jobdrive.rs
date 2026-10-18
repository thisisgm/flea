use std::path::Path;

// st_dev keys ordinary filesystems; btrfs subvolumes use the mount's source because their anonymous devices differ.
pub(super) fn drive_of(target: &Path) -> (String, String) {
    use std::os::unix::fs::MetadataExt;
    let body = super::mountinfo::read_body();
    let mut path = target.to_path_buf();
    while path.symlink_metadata().is_err() && path.pop() {}
    let path = path.canonicalize().unwrap_or(path);
    let entry = super::mountinfo::mount_entry_in(&path, &body);
    let key = if let Some(entry) = entry.as_ref().filter(|e| e.fstype == "btrfs") {
        format!("btrfs:{}", entry.source)
    } else {
        path.metadata()
            .map(|m| format!("dev:{}", m.dev()))
            .unwrap_or_else(|_| {
                format!(
                    "mount:{}",
                    super::iomount::mount_key(&path, &body).display()
                )
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
