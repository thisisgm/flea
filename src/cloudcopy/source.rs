use super::runner::Runner;
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
type Stamp = (u64, u64, u64, i64, i64, i64, i64);
pub type Inventory = BTreeMap<String, Stamp>;

fn local(path: &Path, text: &str) -> Result<(), String> {
    // Refuse remote and virtual sources before opening them. Parsing mountinfo needs no mounted I/O.
    let kind = crate::backend::mountinfo::mount_type_in(path, text)
        .ok_or("Cannot identify source filesystem")?;
    if ![
        "ext4", "ext3", "ext2", "btrfs", "xfs", "f2fs", "vfat", "exfat", "ntfs3", "tmpfs",
        "overlay", "zfs", "bcachefs", "ecryptfs",
    ]
    .contains(&kind.as_str())
    {
        return Err(
            "Source filesystem is unsupported for direct upload (cloud/network mounts are refused)"
                .into(),
        );
    }
    Ok(())
}
pub fn inspect(path: &Path, runner: &Runner) -> Result<(bool, Inventory), String> {
    if path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) {
        return Err("Source must not contain parent traversal".into());
    }
    if !path.is_absolute() {
        return Err("Source must be an absolute local path".into());
    }
    let mounts = std::fs::read_to_string("/proc/self/mountinfo")
        .map_err(|_| "Cannot identify source filesystem")?;
    local(path, &mounts)?;
    for ancestor in path.ancestors() {
        if ancestor
            .symlink_metadata()
            .map_err(|_| "Source is unavailable")?
            .file_type()
            .is_symlink()
        {
            return Err("Symlink sources and symlink parent directories are not supported".into());
        }
    }
    let is_dir = path
        .symlink_metadata()
        .map_err(|_| "Source is unavailable")?
        .is_dir();
    let base = if is_dir {
        path
    } else {
        path.parent().ok_or("Source has no parent")?
    };
    let mut todo: Vec<PathBuf> = vec![path.into()];
    let mut files = BTreeMap::new();
    let mut entries = 0;
    let mut name_bytes = 0;
    while let Some(p) = todo.pop() {
        runner.check()?;
        entries += 1;
        if entries > 100_000 {
            return Err("Direct upload is limited to 100,000 entries per job".into());
        }
        local(&p, &mounts)?;
        let meta = p
            .symlink_metadata()
            .map_err(|_| "Source changed or cannot be read")?;
        if meta.file_type().is_symlink() || !(meta.is_file() || meta.is_dir()) {
            return Err("Symlinks and special files are not supported; originals kept".into());
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(p).map_err(|_| "Cannot read source directory")? {
                if entries + todo.len() >= 100_000 {
                    return Err("Direct upload is limited to 100,000 entries per job".into());
                }
                todo.push(entry.map_err(|_| "Cannot read source entry")?.path());
            }
        } else {
            let name = p
                .strip_prefix(base)
                .unwrap()
                .to_str()
                .ok_or("Non-UTF-8 names are unsupported")?;
            if !super::config::relative(name) {
                return Err("Source contains unsupported names".into());
            }
            name_bytes += name.len();
            if name_bytes > 16 * 1024 * 1024 {
                return Err("Source filename inventory exceeded the safety limit".into());
            }
            files.insert(
                name.into(),
                (
                    meta.dev(),
                    meta.ino(),
                    meta.len(),
                    meta.mtime(),
                    meta.mtime_nsec(),
                    meta.ctime(),
                    meta.ctime_nsec(),
                ),
            );
        }
    }
    if files.is_empty() {
        return Err("No regular files to upload. Empty directories are not copied.".into());
    }
    Ok((is_dir, files))
}
pub fn manifest(body: &str, files: &Inventory) -> Result<(), String> {
    let mut names = std::collections::BTreeSet::new();
    for line in body.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 35
            || !bytes[..32].iter().all(u8::is_ascii_hexdigit)
            || &bytes[32..34] != b"  "
        {
            return Err("A complete MD5 manifest could not be produced".into());
        }
        if !names.insert(line[34..].to_owned()) {
            return Err("Duplicate checksum entry".into());
        }
    }
    if names.len() != files.len() || !files.keys().all(|p| names.contains(p)) {
        return Err("Source manifest does not cover every selected file".into());
    }
    Ok(())
}
