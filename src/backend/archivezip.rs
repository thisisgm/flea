// 7z's zip writer blocks opening a FIFO and libarchive reads a zip's backslash as a separator, so one walk plans each zip first.
use crate::backend::opsreq::op_err;
use crate::error::FleaError;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const ONLY_SPECIAL: &str = "a zip cannot hold a pipe, socket or device file";
// The 7z exclusions ride on argv, so their total stays far under the kernel's argument limit.
const EXCLUDE_BYTES: usize = 256 * 1024;
const EXCLUDE_SWITCH: &str = "-x!";
const TOO_MANY: &str =
    "this selection holds too many pipes, sockets or device files for a zip, choose tar or 7z";

// The selected names a zip takes, and the nested special files 7z is told to leave out by relative path.
pub struct Plan {
    pub names: Vec<String>,
    pub skipped: Vec<String>,
}

fn special(kind: &std::fs::FileType) -> bool {
    kind.is_fifo() || kind.is_socket() || kind.is_char_device() || kind.is_block_device()
}

fn backslash(rel: &Path) -> Result<(), FleaError> {
    if rel.as_os_str().as_bytes().contains(&b'\\') {
        let shown = rel.to_string_lossy();
        return Err(op_err(
            "archive",
            &shown,
            &format!("a zip cannot keep the backslash in {shown}, choose tar or 7z"),
        ));
    }
    Ok(())
}

// A selected special file is dropped; a selection of nothing else is refused rather than written as an empty zip.
pub fn plan(parent: &Path, names: &[String], cancel: &AtomicBool) -> Result<Plan, FleaError> {
    let mut plan = Plan {
        names: Vec::with_capacity(names.len()),
        skipped: Vec::new(),
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for name in names {
        let rel = flea::bytename::exact(name.as_str());
        backslash(&rel)?;
        match parent.join(&rel).symlink_metadata() {
            Ok(meta) if special(&meta.file_type()) => continue,
            Ok(meta) if meta.is_dir() => dirs.push(rel),
            _ => {}
        }
        plan.names.push(name.clone());
    }
    if plan.names.is_empty() {
        return Err(op_err(
            "archive",
            names.first().map_or("", String::as_str),
            ONLY_SPECIAL,
        ));
    }
    let mut bytes = 0;
    while let Some(rel) = dirs.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err(op_err("archive", &rel.to_string_lossy(), "cancelled"));
        }
        // An unreadable folder is left to the tool, which names it in its own failure.
        let Ok(entries) = std::fs::read_dir(parent.join(&rel)) else {
            continue;
        };
        for entry in entries.flatten() {
            let child = rel.join(entry.file_name());
            backslash(&child)?;
            // d_type answers this without a stat; a link is never followed, as 7z's -snl stores it.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                dirs.push(child);
            } else if special(&kind) {
                let Some(text) = child.to_str() else {
                    let shown = child.to_string_lossy();
                    return Err(op_err(
                        "archive",
                        &shown,
                        &format!("{shown} cannot be left out of a zip, choose tar or 7z"),
                    ));
                };
                bytes += EXCLUDE_SWITCH.len() + text.len();
                if bytes > EXCLUDE_BYTES {
                    return Err(op_err("archive", text, TOO_MANY));
                }
                plan.skipped.push(text.to_string());
            }
        }
    }
    Ok(plan)
}

// 7z's exclusion switch per skipped path; -spd keeps each one literal, and a path with a slash matches only itself.
pub fn exclude(argv: &mut Vec<String>, skipped: &[String]) {
    argv.extend(skipped.iter().map(|rel| format!("{EXCLUDE_SWITCH}{rel}")));
}

#[cfg(test)]
#[path = "archivezip_tests.rs"]
mod tests;
