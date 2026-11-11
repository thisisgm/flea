// Each reviewed node's parent, opened by name from the review root every time, a whole stretch of path per call.
use super::*;

// openat2(2) and its RESOLVE_* bits, the same numbers on x86_64 and aarch64.
const SYS_OPENAT2: i64 = 437;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;
const O_CLOEXEC: i32 = 0o2000000;
// The most path one openat2 call is given, under PATH_MAX with room for its NUL.
const STRETCH: usize = 4000;

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

extern "C" {
    fn syscall(number: i64, ...) -> i64;
}

#[cfg(test)]
thread_local! {
    static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// A renamed or swapped folder anywhere on the path refuses, as opening each folder by name did, at one call per stretch of path.
pub(super) fn parent_at(
    base: &File,
    root: &OsStr,
    relative: &Path,
) -> Result<(File, OsString), Refusal> {
    if relative.as_os_str().is_empty() {
        return Ok((
            base.try_clone()
                .changed("Could not duplicate a Trash handle")?,
            root.to_os_string(),
        ));
    }
    let mut names = vec![root];
    for component in relative
        .parent()
        .changed("Missing Trash review parent.")?
        .components()
    {
        let std::path::Component::Normal(name) = component else {
            return Err(Refusal::changed("Invalid Trash review parent."));
        };
        names.push(name);
    }
    let name = relative
        .file_name()
        .changed("Missing Trash review name.")?
        .to_os_string();
    Ok((open_beneath(base, &names)?, name))
}

// Opens base/names... a stretch at a time, each stretch from the folder the last one opened.
fn open_beneath(base: &File, names: &[&OsStr]) -> Result<File, Refusal> {
    let mut dir: Option<File> = None;
    let mut stretch = PathBuf::new();
    for (at, name) in names.iter().enumerate() {
        stretch.push(name);
        let last = at + 1 == names.len();
        if last || stretch.as_os_str().len() + names[at + 1].len() + 1 > STRETCH {
            let from = dir.as_ref().unwrap_or(base);
            dir = Some(open_stretch(from, &stretch).map_err(|e| {
                Refusal::os(
                    Outcome::Changed,
                    &format!(
                        "Could not open Trash folder {}",
                        fd_path(from, stretch.as_os_str()).display()
                    ),
                    &e,
                )
            })?);
            stretch = PathBuf::new();
        }
    }
    dir.changed("Missing Trash review parent.")
}

// No symlink on the path and nothing outside `from`; every kernel Omarchy ships has openat2 (Linux 5.6 on).
fn open_stretch(from: &File, stretch: &Path) -> std::io::Result<File> {
    #[cfg(test)]
    CALLS.with(|calls| calls.set(calls.get() + 1));
    let path = CString::new(stretch.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(ErrorKind::InvalidInput))?;
    let how = OpenHow {
        flags: (O_NOFOLLOW | crate::oflags::O_DIRECTORY | O_CLOEXEC) as u64,
        mode: 0,
        resolve: RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH,
    };
    let fd = unsafe {
        syscall(
            SYS_OPENAT2,
            from.as_raw_fd() as i64,
            path.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { std::os::unix::io::FromRawFd::from_raw_fd(fd as i32) })
}

#[cfg(test)]
#[path = "parents_tests.rs"]
mod tests;
