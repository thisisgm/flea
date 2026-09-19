use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
extern "C" {
    fn geteuid() -> u32;
    fn flock(fd: i32, operation: i32) -> i32;
}
fn private(path: &Path) -> Result<(), String> {
    let meta = path
        .symlink_metadata()
        .map_err(|_| "Upload lock directory unavailable")?;
    if !meta.is_dir() || meta.uid() != unsafe { geteuid() } || meta.mode() & 0o077 != 0 {
        return Err("Upload lock directory must be private to this user".into());
    }
    Ok(())
}
// Coarse by configured remote name: separate windows must not race destination checks.
pub fn acquire(remote: &str) -> Result<File, String> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or("A private XDG_RUNTIME_DIR is required")?;
    if !runtime.is_absolute() {
        return Err("Runtime directory must be absolute".into());
    }
    private(&runtime)?;
    let directory = runtime.join("flea-cloud-copy");
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err("Cannot create upload lock directory".into()),
    }
    private(&directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(crate::oflags::O_NOFOLLOW | 0o4000)
        .open(directory.join(format!("{remote}.lock")))
        .map_err(|_| "Cannot open upload lock")?;
    let meta = file.metadata().map_err(|_| "Cannot inspect upload lock")?;
    if !meta.is_file() || meta.uid() != unsafe { geteuid() } || meta.mode() & 0o077 != 0 {
        return Err("Upload lock is not private".into());
    }
    if unsafe { flock(file.as_raw_fd(), 6) } != 0 {
        return Err("Another Flea upload is using this remote. Wait for it to finish.".into());
    }
    Ok(file)
}
