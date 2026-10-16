use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub fn root(variable: &str, fallback: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(variable).map(PathBuf::from).filter(|p| p.is_absolute()) { return Ok(path); }
    let home = std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute()).ok_or("The user home is unavailable.")?;
    Ok(home.join(fallback))
}
pub fn read(path: &Path) -> Result<Option<String>, String> {
    let file = match OpenOptions::new().read(true).custom_flags(0x800).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 { return Err("The preference file must be a regular file smaller than 1 MiB.".into()); }
    let mut text = String::new();
    file.take(1024 * 1024 + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
    if text.len() > 1024 * 1024 { return Err("The preference file exceeds 1 MiB.".into()); }
    Ok(Some(text))
}
pub fn mode(path: &Path) -> u32 { fs::metadata(path).map(|m| m.permissions().mode() & 0o7777).unwrap_or(0o600) }

// Resolve dotfile links, including a missing filename below an existing linked directory.
pub fn resolve(path: &Path) -> Result<PathBuf, String> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() { return Err("The configuration contains a broken symlink.".into()); }
            let parent = path.parent().ok_or("Invalid preference path.")?;
            let name = path.file_name().ok_or("Invalid preference path.")?;
            Ok(resolve(parent)?.join(name))
        }
        Err(e) => Err(e.to_string()),
    }
}
struct Temp(PathBuf);
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_file(&self.0); } }
pub fn atomic(path: &Path, text: &str, mode: u32) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid preference path.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    for i in 0..100 {
        let temp = parent.join(format!(".flea-font-{}-{i}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp) {
            Ok(mut file) => {
                let temp = Temp(temp);
                file.set_permissions(fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
                file.write_all(text.as_bytes()).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
                fs::rename(&temp.0, path).map_err(|e| e.to_string())?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Could not prepare the preference file.".into())
}
pub fn write(path: &Path, text: Option<&str>, mode: u32) -> Result<(), String> {
    if let Some(text) = text { atomic(path, text, mode) }
    else { match fs::remove_file(path) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(e.to_string()) } }
}
pub fn executable(name: &str) -> bool {
    std::env::var_os("PATH").map(|paths| std::env::split_paths(&paths).any(|p| {
        fs::metadata(p.join(name)).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    })).unwrap_or(false)
}
extern "C" { fn flock(fd: i32, operation: i32) -> i32; }
pub fn lock(state: &Path) -> Result<File, String> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(state).map_err(|e| e.to_string())?;
    let file = OpenOptions::new().write(true).create(true).custom_flags(0x20000 | 0x800).mode(0o600).open(state.join(".lock")).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() { return Err("Invalid preference lock.".into()); }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if unsafe { flock(file.as_raw_fd(), 2 | 4) } == 0 { return Ok(file); }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock { return Err(error.to_string()); }
        if std::time::Instant::now() >= deadline { return Err("Another font preference action is still running.".into()); }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
