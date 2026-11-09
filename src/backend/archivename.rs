// A taken compress or extract output lands under Duplicate's next free " copy" name, never refused or merged into.
use crate::backend::ops::NAME_TRIES;
use crate::backend::opsreq::op_err;
use crate::error::FleaError;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

// Sample: "pack.tar.gz" in format "tar.gz" steps to "pack copy.tar.gz", then "pack copy 2.tar.gz".
// A folder (format None) keeps its whole name; an archive name without the format's tail splits as Duplicate's does.
pub(crate) fn candidate(dest: &Path, format: Option<&str>, n: usize) -> Option<PathBuf> {
    let name = dest.file_name()?.to_str()?;
    let suffix = if n <= 1 {
        " copy".to_string()
    } else {
        format!(" copy {}", n)
    };
    let named = match format {
        Some(format) => match name
            .strip_suffix(&format!(".{}", format))
            .filter(|stem| !stem.is_empty())
        {
            Some(stem) => format!("{}{}.{}", stem, suffix, format),
            None => crate::backend::ops::copy_name(dest, "copy", n)?,
        },
        None => format!("{}{}", name, suffix),
    };
    Some(dest.with_file_name(named))
}

// A name that is not UTF-8 has no copy name, as Duplicate makes none for it either.
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
