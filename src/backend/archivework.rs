// The staging directory every delegated archive job writes into, the jail those jobs run in, and the
// two reads an extract is verified against. The jobs themselves are archiveops.rs.
use crate::backend::sandbox;
use crate::backend::archive::Formats;
use crate::backend::archivelist::parse_reader;
use crate::backend::opsreq::op_err;
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};
use std::process::Command;

// A private directory beside the destination, so the rename that follows never crosses a filesystem.
const WORK_PREFIX: &str = ".flea-work-";

pub struct Work {
    pub dir: PathBuf,
}

// Archive and convert run concurrently by design, so the pid alone does not name a job: two of them
// beside the same destination would claim one path, and the second's cleanup would destroy the
// first's in-flight output. The counter is what makes a name belong to one job.
static WORK_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// A name already taken means a leftover from a killed run, and stepping past it is bounded so a
// directory full of them cannot spin.
const WORK_ATTEMPTS: usize = 64;

impl Work {
    // create_dir, not create_dir_all: a name already taken is a collision and must never merge, and
    // create_new semantics are also what stops this from adopting somebody else's live directory.
    pub fn new(beside: &Path, tag: &str) -> Result<Work, FleaError> {
        let mut last = String::new();
        for _ in 0..WORK_ATTEMPTS {
            let seq = WORK_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = beside.join(format!("{}{}-{}-{}", WORK_PREFIX, tag, std::process::id(), seq));
            match std::fs::create_dir(&dir) {
                Ok(()) => return Ok(Work { dir }),
                // Nothing is ever removed here: a name in use may be a live sibling's, and the only
                // safe answer to a taken name is a different name.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    last = dir.to_string_lossy().to_string();
                }
                Err(e) => return Err(from_io("archive", &dir.to_string_lossy(), &e)),
            }
        }
        Err(op_err("archive", &last, "no free work directory beside the destination"))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        // Only ever a directory this process made, under a name only this module writes.
        if self.dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with(WORK_PREFIX)) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

// The tools print their own diagnosis on stderr and do not always exit non-zero, so success is read
// off the filesystem: the file the job was told to produce either exists afterwards or it does not.
pub fn run_boxed(inner: Vec<String>, read_only: &Path, writable: &Path) -> Result<(), FleaError> {
    // Fail closed: the jail is the only containment for these tools, so a missing bwrap or prlimit
    // refuses the job rather than running it unsandboxed, the same rule thumbs.rs already follows.
    if !sandbox::available() {
        let tool = inner.first().map_or("", |s| s.as_str());
        return Err(op_err("archive", tool, "the sandbox is unavailable: bwrap or prlimit is not on PATH"));
    }
    let full = sandbox::wrap(&inner, read_only, writable);
    let out = Command::new(&full[0])
        .args(&full[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .output()
        .map_err(|e| from_io("archive", &full[0], &e))?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stderr);
    Err(op_err("archive", "", text.lines().last().unwrap_or("the archive tool failed")))
}


pub fn is_empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|mut e| e.next().is_none()).unwrap_or(true)
}

// How many members the index names that should have produced something in the destination, per
// Row::produces_destination_entry, or None when the index could not be read at all. None is the
// honest answer for a listing that failed, timed out or was truncated, because a count of zero from a
// read that never finished is indistinguishable from an archive holding nothing, and reading the
// first as the second is what restored the defect this check exists for.
pub fn archive_produced_count(formats: &Formats, archive: &Path) -> Option<usize> {
    let (inner, spec) = formats.list_argv(archive)?;
    if !sandbox::available() {
        return None;
    }
    let full = sandbox::wrap_readonly(&inner, archive);
    // Streamed, not .output(): buffering the whole index here would contradict the streaming
    // contract the parser exists for, and a 200k-entry archive is exactly the case that motivated it.
    let mut child = Command::new(&full[0])
        .args(&full[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let listed = match child.stdout.take() {
        Some(out) => parse_reader(std::io::BufReader::new(out), &spec),
        None => {
            let _ = child.wait();
            return None;
        }
    };
    let status = child.wait().ok()?;
    if !status.success() || listed.failed {
        return None;
    }
    Some(listed.produced_entries)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn a_work_directory_is_made_beside_the_destination_and_goes_with_its_own_drop() {
        let d = TestDir::new("archwork");
        let kept;
        {
            let w = Work::new(d.path(), "arc").expect("work");
            kept = w.dir.clone();
            assert!(kept.is_dir());
            assert!(kept.file_name().unwrap().to_string_lossy().starts_with(WORK_PREFIX));
            // Beside the destination, so the rename that follows never crosses a filesystem.
            assert_eq!(kept.parent().unwrap(), d.path());
        }
        assert!(!kept.exists(), "the work directory goes with the job that made it");
    }

    #[test]
    fn two_work_directories_beside_the_same_destination_never_share_a_path() {
        let d = TestDir::new("archwork2");
        let first = Work::new(d.path(), "ext").expect("first");
        let second = Work::new(d.path(), "ext").expect("second");
        assert_ne!(first.dir, second.dir, "a second job must not claim the first job's directory");
        assert!(first.dir.is_dir(), "and must not have destroyed it");
        assert!(second.dir.is_dir());
        // In flight, so a live sibling's contents have to survive the other one being created.
        std::fs::write(first.dir.join("in-flight"), b"payload").expect("write");
        let third = Work::new(d.path(), "ext").expect("third");
        assert!(first.dir.join("in-flight").is_file(), "a third job must not destroy either");
        assert_ne!(third.dir, first.dir);
        assert_ne!(third.dir, second.dir);
    }
}
