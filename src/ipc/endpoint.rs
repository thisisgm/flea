use std::fs::{self, DirBuilder, Metadata};
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

extern "C" {
    fn geteuid() -> u32;
    fn kill(pid: i32, signal: i32) -> i32;
}

pub fn uid() -> u32 {
    // geteuid has no arguments or failure case.
    unsafe { geteuid() }
}

fn private(metadata: &Metadata) -> bool {
    metadata.uid() == uid() && metadata.mode() & 0o077 == 0
}

fn refuse(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

pub fn runtime() -> io::Result<PathBuf> {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| refuse("XDG_RUNTIME_DIR is required for window IPC"))?;
    validate_runtime(&root)?;
    Ok(root)
}

pub fn validate_runtime(root: &Path) -> io::Result<()> {
    if !root.is_absolute() {
        return Err(refuse("XDG_RUNTIME_DIR must be absolute"));
    }
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || !private(&metadata) {
        return Err(refuse("XDG_RUNTIME_DIR must be an owner-only directory"));
    }
    Ok(())
}

pub fn directory(root: &Path) -> io::Result<PathBuf> {
    validate_runtime(root)?;
    let directory = root.join("flea-ipc");
    match DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error),
    }
    check_directory(&directory)?;
    Ok(directory)
}

fn check_directory(directory: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || !private(&metadata) {
        return Err(refuse("the IPC directory must be an owner-only directory"));
    }
    Ok(())
}

fn socket_metadata(path: &Path) -> io::Result<Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() || !private(&metadata) {
        return Err(refuse("the IPC endpoint must be an owner-only socket"));
    }
    Ok(metadata)
}

pub fn alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // Signal zero checks existence without delivering a signal; EPERM still names a live process.
    unsafe { kill(pid as i32, 0) == 0 || io::Error::last_os_error().raw_os_error() != Some(3) }
}

fn pid_of(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?.strip_suffix(".sock")?;
    let pid = name.parse::<u32>().ok()?;
    (pid.to_string() == name && pid > 0 && pid <= i32::MAX as u32).then_some(pid)
}

pub fn prune(directory: &Path) -> io::Result<()> {
    check_directory(directory)?;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let Some(pid) = pid_of(&path) else { continue };
        let Ok(before) = socket_metadata(&path) else {
            continue;
        };
        if alive(pid) {
            continue;
        }
        if let Err(error) = super::connect::with_timeout(&path, Duration::ZERO) {
            if error.kind() == io::ErrorKind::ConnectionRefused && !alive(pid) {
                if let Ok(after) = socket_metadata(&path) {
                    if before.ino() == after.ino() && before.dev() == after.dev() {
                        match fs::remove_file(&path) {
                            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                                return Err(error)
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn instances(root: &Path) -> io::Result<Vec<(u32, PathBuf)>> {
    validate_runtime(root)?;
    let directory = root.join("flea-ipc");
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    check_directory(&directory)?;
    let mut sockets = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let Some(pid) = pid_of(&path) else { continue };
        if let Ok(metadata) = socket_metadata(&path) {
            if alive(pid) {
                sockets.push((metadata.mtime(), metadata.mtime_nsec(), pid, path));
            }
        }
    }
    sockets.sort();
    Ok(sockets
        .into_iter()
        .map(|(_, _, pid, path)| (pid, path))
        .collect())
}

pub struct Endpoint {
    pub listener: UnixListener,
    pub path: PathBuf,
    identity: Metadata,
}

impl Endpoint {
    pub fn bind(root: &Path) -> io::Result<Self> {
        let directory = directory(root)?;
        prune(&directory)?;
        let path = directory.join(format!("{}.sock", std::process::id()));
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let identity = socket_metadata(&path)?;
        Ok(Self {
            listener,
            path,
            identity,
        })
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        if let Ok(metadata) = socket_metadata(&self.path) {
            if metadata.ino() == self.identity.ino() && metadata.dev() == self.identity.dev() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
