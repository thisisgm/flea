// One compress, one extract, one convert: what a delegated archive job actually does, each staged
// into a private directory and renamed into place. The wire and the request plumbing are
// archivereq.rs's job, the staging and the jail are archivework.rs's, and reading an index is
// archivelist.rs's.
use crate::backend::archive::Formats;
use crate::backend::archiveenc;
use crate::backend::archivefree;
use crate::backend::archivepass;
use crate::backend::archivework::{archive_produced_count_cancellable, is_empty_dir,
                                   run_boxed_cancellable, run_boxed_cancellable_capped,
                                   run_boxed_cancellable_with, Work};
use crate::backend::convert;
use crate::backend::ops::rename_noreplace;
use crate::backend::opsreq::op_err;
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// The sentence both the request and the job refuse a disk image extract with.
pub const DISK_IMAGE_REFUSAL: &str = "a disk image opens as a listing and is never extracted";

// A password compress is a 7z job whatever the format: bsdtar writes no encryption at all.
pub struct Password<'a> {
    pub secret: &'a str,
    pub encrypt_names: bool,
}

/// One archive out of a selection that all shares a parent, which is what a listing selection is.
pub fn compress(formats: &Formats, parent: &Path, names: &[String], format: &str,
                dest: &Path, cancel: &AtomicBool, password: Option<Password>) -> Result<(), FleaError> {
    if dest.symlink_metadata().is_ok() {
        return Err(op_err("archive", &dest.to_string_lossy(), "that destination already exists"));
    }
    // A quit landing between registration and this check never starts a child.
    if cancel.load(Ordering::Relaxed) {
        return Err(op_err("archive", &dest.to_string_lossy(), "cancelled"));
    }
    if password.is_some() {
        if !formats.has_7z() {
            return Err(op_err("archive", format, "a password needs 7-Zip, which is not installed"));
        }
        if format != "7z" && format != "zip" {
            return Err(op_err("archive", format, "a password needs the 7z or zip format"));
        }
        if !formats.offers(format) {
            return Err(op_err("archive", format, "no installed tool can create that format"));
        }
    }
    let mut work = Work::new(parent, "arc")?;
    let staged = work.dir.join(format!("archive.{}", format));
    // The password arm builds its own 7z argv and runs it in the archive jail beside every other archive job.
    if let Some(password) = password {
        let inner = archivepass::compress_argv(format, &staged, parent, names, password.encrypt_names);
        match archivepass::run_boxed_cancellable_password("archive", inner, parent, &mut work, Some(password.secret), cancel)? {
            None => {}
            Some(f) => return Err(op_err("archive", format, &f.message)),
        }
    } else {
        let inner = match formats.compress_argv(format, &staged, parent, names) {
            Some(a) => a,
            None => return Err(op_err("archive", format, "no installed tool can create that format")),
        };
        match formats.user_program(format) {
            Some(program) => run_boxed_cancellable_with("archive", inner, parent, program, &mut work, cancel)?,
            None => run_boxed_cancellable("archive", inner, parent, &mut work, cancel)?,
        }
    }
    if staged.symlink_metadata().is_err() {
        return Err(op_err("archive", format, "the archive tool wrote nothing"));
    }
    rename_noreplace(&staged, dest)
}

// Staged like compress and convert: the tool writes into a private work directory and the finished
// tree is renamed into place, so a failed or half-done extract never leaves a partial destination.
// Ok(true) is a verified success and Ok(false) one this could not check, which is a real difference
// to the operator: three rounds of this branch went into an empty directory published as a success,
// and publishing an unverified one as an ordinary success is a quieter version of the same thing.
// cancel stops the job, kills its child and publishes nothing; the Work guard drops the stage.
pub fn extract(
    formats: &Formats,
    archive: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    password: Option<&str>,
) -> Result<bool, FleaError> {
    if dest.symlink_metadata().is_ok() {
        return Err(op_err("archive", &dest.to_string_lossy(), "that destination already exists"));
    }
    // A disk image lists and is never extracted, so it is refused before any staging or probing.
    if Formats::is_list_only(archive) {
        return Err(op_err("archive", &archive.to_string_lossy(), DISK_IMAGE_REFUSAL));
    }
    let leaf = archive.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if password.is_some() && !archivepass::reads_password(formats, archive) {
        return Err(op_err("archive", &leaf, archivepass::NO_PASSWORD_READER));
    }
    // A cancel that landed in the slot wait stops here, and an encrypted archive asks for its password before any extract starts.
    if password.is_none() {
        if cancel.load(Ordering::Relaxed) {
            return Err(op_err("archive", &archive.to_string_lossy(), "cancelled"));
        }
        let needed = archiveenc::password_needed(archive, formats, cancel);
        // A cancel during the probe killed it, so its None is no verdict and the job is cancelled.
        if cancel.load(Ordering::Relaxed) {
            return Err(op_err("archive", &archive.to_string_lossy(), "cancelled"));
        }
        if needed == Some(true) {
            return Err(op_err("archive", &leaf, archivepass::ENCRYPTED));
        }
    }
    // The free-space refusal lands before any child runs; an unreadable index refuses nothing.
    if let Some(unpacked) = archivepass::unpacked_bytes(archive, formats, cancel) {
        if let Some(sentence) = archivefree::check(&leaf, unpacked, dest) {
            return Err(op_err("archive", &archive.to_string_lossy(), &sentence));
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(op_err("archive", &archive.to_string_lossy(), "cancelled"));
    }
    let parent = dest.parent().unwrap_or(Path::new("/"));
    let mut work = Work::new(parent, "ext")?;
    let staged = work.dir.join("out");
    std::fs::create_dir(&staged).map_err(|e| from_io("archive", &staged.to_string_lossy(), &e))?;
    let argv = match password {
        Some(_) => archivepass::extract_argv(formats, archive, &staged),
        None => formats.extract_argv(archive, &staged),
    };
    let inner = match argv {
        Some(a) => a,
        None => return Err(op_err("archive", &archive.to_string_lossy(), "no installed tool can open that archive")),
    };
    // A cancel that landed while the job waited for the slot aborts before any child runs.
    if cancel.load(Ordering::Relaxed) {
        return Err(op_err("archive", &archive.to_string_lossy(), "cancelled"));
    }
    if archivepass::runs_on_seven(formats, archive, password) {
        if let Some(f) = archivepass::run_boxed_cancellable_password("archive", inner, archive, &mut work, password, cancel)? {
            let said = archivepass::extract_verdict(f.code, password.is_some(), || archiveenc::password_needed(archive, formats, cancel));
            // A cancel during the guarded re-probe killed it; the job is cancelled, whatever the extract's exit said.
            if cancel.load(Ordering::Relaxed) {
                return Err(op_err("archive", &archive.to_string_lossy(), "cancelled"));
            }
            return Err(op_err("archive", &leaf, said.unwrap_or(&f.message)));
        }
    } else {
        run_boxed_cancellable("archive", inner, archive, &mut work, cancel)?;
    }
    // compress and convert stat a path Flea never creates, so their existence check is a real test.
    // This one creates its own staging directory, so the same shape always passes. Two archives
    // legally extract to nothing: an empty one, and one whose only member is the archive root, which
    // is what `tar -c -C <empty dir> .` produces.
    // THE INVARIANT, stated before the code because three predicates in a row were each a correct
    // reaction to the counter-example in front of them and each opened a different hole: an extract
    // succeeded when everything the index said should appear in the destination did. A member
    // produces a destination entry unless it IS the destination, which is the archive root and
    // nothing else, so "./" produces none while "./a/" produces one even though both are
    // directories. Counting entries missed the root; counting files missed nested directories.
    let mut verified = true;
    if is_empty_dir(&staged) {
        match archive_produced_count_cancellable(formats, archive, cancel)? {
            // The index named something and nothing arrived: the tool exited 0 having written nothing.
            Some(n) if n > 0 => {
                return Err(op_err("archive", &archive.to_string_lossy(), "the archive tool wrote nothing"));
            }
            // Nothing to extract, so an empty destination is the correct result.
            Some(_) => {}
            // The index could not be read, so this cannot be judged. Refusing would punish the
            // operator for Flea's own verification failing, including for our own deadline, and an
            // unverifiable check is not evidence of failure. It is published and SAID to be
            // unverified, because a success nobody checked must not read as one that was checked.
            // corner: a tool that lies AND an unreadable index at once publishes an empty directory.
            None => verified = false,
        }
    }
    // A cancel during the index read still discards; a cancelled job publishes nothing.
    if cancel.load(Ordering::Relaxed) {
        return Err(op_err("archive", &archive.to_string_lossy(), "cancelled"));
    }
    rename_noreplace(&staged, dest)?;
    Ok(verified)
}

pub fn convert_one(input: &Path, dest: &Path, strip: bool, cancel: &AtomicBool) -> Result<(), FleaError> {
    if dest.symlink_metadata().is_ok() {
        return Err(op_err("convert", &dest.to_string_lossy(), "that destination already exists"));
    }
    // Absolute, so ImageMagick can never read the input as an option (a file named "-write ...").
    // The staged destination is already absolute under the work directory beside dest.
    let input = std::fs::canonicalize(input).map_err(|e| from_io("convert", &input.to_string_lossy(), &e))?;
    let input = input.as_path();
    // A quit sets this flag; the converter keeps its CPU cap on the cancellable runner.
    if cancel.load(Ordering::Relaxed) {
        return Err(op_err("convert", &dest.to_string_lossy(), "cancelled"));
    }
    let parent = dest.parent().unwrap_or(Path::new("/"));
    let mut work = Work::new(parent, "cvt")?;
    let name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let staged = work.dir.join(&name);
    run_boxed_cancellable_capped("convert", convert::argv(input, &staged, strip), input, &mut work, cancel)?;
    if staged.symlink_metadata().is_err() {
        return Err(op_err("convert", &name, "the converter wrote nothing"));
    }
    rename_noreplace(&staged, dest)
}

// A compress names absolute paths, which all share a parent because a selection comes from one
// listing; the parent and the relative names are derived here rather than sent twice on the wire.
pub fn split_paths(paths: &[String]) -> Option<(PathBuf, Vec<String>)> {
    let first = Path::new(paths.first()?);
    let parent = first.parent()?.to_path_buf();
    let mut names = Vec::with_capacity(paths.len());
    for p in paths {
        let path = Path::new(p);
        // A path from another directory would be stored under a name that is not its own, so it is refused.
        if path.parent() != Some(parent.as_path()) {
            return None;
        }
        names.push(path.file_name()?.to_string_lossy().to_string());
    }
    Some((parent, names))
}

#[cfg(test)]
#[path = "archiveops_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "archiveopscancel_tests.rs"]
mod cancel_tests;
