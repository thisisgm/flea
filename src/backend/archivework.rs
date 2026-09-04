// The staging directory every delegated archive job writes into, the jail those jobs run in, and the
// two reads an extract is verified against. The jobs themselves are archiveops.rs.
use crate::backend::archive::Formats;
use crate::backend::archive_reader;
use crate::backend::opsreq::op_err;
use crate::backend::sandbox;
use crate::error::{from_io, FleaError};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
    // Fail closed: the jail is the only containment for these tools, so a missing sandbox program
    // refuses the job rather than running it unsandboxed, the same rule thumbs.rs already follows.
    if !sandbox::available() {
        let tool = inner.first().map_or("", |s| s.as_str());
        return Err(op_err("archive", tool, "the sandbox is unavailable: bwrap, prlimit, or systemd-run is not on PATH"));
    }
    let full = sandbox::one_shot(&sandbox::wrap(&inner, read_only, writable));
    let out = captured_output(&full).map_err(|e| from_io("archive", &full[0], &e))?;
    if out.status.success() {
        return Ok(());
    }
    Err(op_err("archive", "", &last_error(&out)))
}

fn captured_output(argv: &[String]) -> io::Result<Output> {
    Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).output()
}

fn last_error(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or("the archive tool failed").to_string()
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
    let full = sandbox::one_shot(&sandbox::wrap_readonly(&inner, archive));
    let listed = archive_reader::run(&full, &spec);
    if listed.failed {
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

    #[test]
    fn archive_commands_keep_final_status_and_the_last_stderr_line() {
        let argv = ["/usr/bin/sh".to_string(), "-c".to_string(), "printf 'first\\nfinal reason\\n' >&2; exit 7".to_string()];
        let output = captured_output(&argv).unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(last_error(&output), "final reason");
        assert!(output.stdout.is_empty(), "archive tool stdout stays discarded");
    }
}
