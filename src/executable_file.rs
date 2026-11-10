//! Publish executables without a writable destination descriptor in the host.
use std::ffi::CString;
use std::fs::{self, DirBuilder};
use std::io::{self, Read};
use std::net::Shutdown;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::os::unix::{
    ffi::OsStrExt,
    fs::{DirBuilderExt, OpenOptionsExt},
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

// Linux values, declared here because libc is only a dev-dependency of this crate.
const AT_FDCWD: i32 = -100;
const RENAME_NOREPLACE: u32 = 1;
const EINVAL: i32 = 22;
const ENOSYS: i32 = 38;
const EOPNOTSUPP: i32 = 95;

extern "C" {
    fn renameat2(
        from_dir: i32,
        from: *const std::ffi::c_char,
        to_dir: i32,
        to: *const std::ffi::c_char,
        flags: u32,
    ) -> i32;
}

/// Replace an executable atomically, after its isolated writer has exited.
pub fn write(path: &Path, mut bytes: &[u8], mode: u32) -> io::Result<()> {
    let length = bytes.len() as u64;
    publish(path, &mut bytes, length, mode, true)
        .map(|_| ())
        .map_err(|error| named(path, error))
}

/// Copy exactly `length` bytes, refusing an existing destination or a changed source.
pub fn copy_new(path: &Path, from: &mut impl Read, length: u64, mode: u32) -> io::Result<u64> {
    publish(path, from, length, mode, false).map_err(|error| named(path, error))
}

fn named(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("body"));
        let _ = fs::remove_file(self.0.join("errors"));
        let _ = fs::remove_dir(&self.0);
    }
}

struct Writer(Child);

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn publish(
    path: &Path,
    from: &mut impl Read,
    length: u64,
    mode: u32,
    replace: bool,
) -> io::Result<u64> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = parent.join(format!(
        ".flea-executable-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    DirBuilder::new().mode(0o700).create(&directory)?;
    let temporary = Temporary(directory);
    let body = temporary.0.join("body");
    let errors_path = temporary.0.join("errors");
    // A regular diagnostic file has no pipe EOF for an unrelated fork to postpone.
    let errors = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&errors_path)?;
    let (mut input, reader) = UnixStream::pair()?;
    // CLOEXEC still leaves a writer in a forked child until exec; only this isolated child opens the file.
    let mut writer = Writer(
        Command::new("/bin/sh")
            .args(["-c", "set -C; mask=$(umask); /bin/cat > \"$1\" && /bin/chmod -- \"$(printf '%o' \"$(($2 & ~$mask))\")\" \"$1\"", "sh"])
            .arg(&body)
            .arg(mode.to_string())
            .stdin(Stdio::from(OwnedFd::from(reader)))
            .stdout(Stdio::null())
            .stderr(Stdio::from(errors))
            .spawn()?,
    );
    let copied = io::copy(&mut from.take(length.saturating_add(1)), &mut input);
    let closed = input.shutdown(Shutdown::Write);
    drop(input);
    let status = writer.0.wait()?;
    if !status.success() {
        let mut errors = Vec::new();
        fs::File::open(errors_path)?
            .take(8192)
            .read_to_end(&mut errors)?;
        return Err(io::Error::other(format!(
            "executable writer exited with {status}: {}",
            String::from_utf8_lossy(&errors).trim()
        )));
    }
    let copied = copied?;
    closed?;
    if copied != length {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file changed while it was copied",
        ));
    }
    if replace {
        fs::rename(&body, path)?;
    } else {
        rename_new(&body, path)?;
    }
    Ok(copied)
}

fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    let from_c = CString::new(from.as_os_str().as_bytes())?;
    let to_c = CString::new(to.as_os_str().as_bytes())?;
    // RENAME_NOREPLACE preserves an exclusive install even on filesystems without hard links.
    let status = unsafe {
        renameat2(
            AT_FDCWD,
            from_c.as_ptr(),
            AT_FDCWD,
            to_c.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    let renamed = if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    };
    exclusive_fallback(renamed, from, to)
}

// A filesystem without RENAME_NOREPLACE still refuses an existing name through link(2).
fn exclusive_fallback(renamed: io::Result<()>, from: &Path, to: &Path) -> io::Result<()> {
    match renamed {
        Err(error) if matches!(error.raw_os_error(), Some(EINVAL | ENOSYS | EOPNOTSUPP)) => {
            fs::hard_link(from, to)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests;
