// A taken compress or extract output lands under Duplicate's next free " copy" name, never refused or merged into.
use crate::backend::ops::NAME_TRIES;
use crate::backend::opsreq::op_err;
use crate::error::FleaError;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

// Sample: "pack.tar.gz" in format "tar.gz" steps to "pack copy.tar.gz", then "pack copy 2.tar.gz".
// A folder (format None) keeps its whole name; an archive name without the format's tail splits as Duplicate's does.
pub(crate) fn candidate(dest: &Path, format: Option<&str>, n: usize) -> Option<PathBuf> {
    let name = dest.file_name()?;
    let suffix = if n <= 1 {
        " copy".to_string()
    } else {
        format!(" copy {}", n)
    };
    // The name's own bytes carry the suffix, so a name that is not UTF-8 steps to its copy names as Duplicate's do.
    let joined = |stem: &OsStr, tail: Option<&str>| {
        let mut out = OsString::from(stem);
        out.push(&suffix);
        if let Some(tail) = tail {
            out.push(".");
            out.push(tail);
        }
        out
    };
    let named = match format {
        Some(format) => match name
            .as_bytes()
            .strip_suffix(format!(".{}", format).as_bytes())
            .filter(|stem| !stem.is_empty())
        {
            Some(stem) => joined(OsStr::from_bytes(stem), Some(format)),
            None => crate::backend::ops::copy_name(dest, "copy", n)?,
        },
        None => joined(name, None),
    };
    Some(dest.with_file_name(named))
}

// A path with no last name (/ or one ending in ..) has no copy name.
pub(crate) const NO_COPY_NAME: &str = "that name is taken and no copy name can be made from it";

// The rename refuses a taken name, so a name taken between the check and the rename is passed over too.
pub(crate) fn publish(
    staged: &Path,
    dest: &Path,
    format: Option<&str>,
    cancel: &AtomicBool,
    rename: impl Fn(&Path, &Path, &AtomicBool) -> Result<(), FleaError>,
) -> Result<PathBuf, FleaError> {
    for n in 0..=NAME_TRIES {
        let target = match n {
            0 => dest.to_path_buf(),
            n => match candidate(dest, format, n) {
                Some(target) => target,
                None => return Err(op_err("archive", &dest.to_string_lossy(), NO_COPY_NAME)),
            },
        };
        if target.symlink_metadata().is_ok() {
            continue;
        }
        match rename(staged, &target, cancel) {
            Err(error) if error.msg == "already exists" && target.symlink_metadata().is_ok() => {}
            outcome => return outcome.map(|()| target),
        }
    }
    let what = if format.is_some() {
        "archive"
    } else {
        "folder"
    };
    Err(op_err(
        "archive",
        &dest.to_string_lossy(),
        &format!("every copy name for this {} is already taken", what),
    ))
}
