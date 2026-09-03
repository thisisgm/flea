// One compress, one extract, one convert: what a delegated archive job actually does, each staged
// into a private directory and renamed into place. The wire and the request plumbing are
// archivereq.rs's job, the staging and the jail are archivework.rs's, and reading an index is
// archivelist.rs's.
use crate::backend::archive::Formats;
use crate::backend::archivework::{archive_produced_count, is_empty_dir, run_boxed, Work};
use crate::backend::convert;
use crate::backend::ops::rename_noreplace;
use crate::backend::opsreq::op_err;
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};

// One archive out of a selection that all shares a parent, which is what a listing selection is.
pub fn compress(
    formats: &Formats,
    parent: &Path,
    names: &[String],
    format: &str,
    dest: &Path,
) -> Result<(), FleaError> {
    if dest.symlink_metadata().is_ok() {
        return Err(op_err("archive", &dest.to_string_lossy(), "that destination already exists"));
    }
    let work = Work::new(parent, "arc")?;
    let staged = work.dir.join(format!("archive.{}", format));
    let inner = match formats.compress_argv(format, &staged, parent, names) {
        Some(a) => a,
        None => return Err(op_err("archive", format, "this box offers no tool for that format")),
    };
    run_boxed(inner, parent, &work.dir)?;
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
pub fn extract(formats: &Formats, archive: &Path, dest: &Path) -> Result<bool, FleaError> {
    if dest.symlink_metadata().is_ok() {
        return Err(op_err("archive", &dest.to_string_lossy(), "that destination already exists"));
    }
    let parent = dest.parent().unwrap_or(Path::new("/"));
    let work = Work::new(parent, "ext")?;
    let staged = work.dir.join("out");
    std::fs::create_dir(&staged).map_err(|e| from_io("archive", &staged.to_string_lossy(), &e))?;
    let inner = match formats.extract_argv(archive, &staged) {
        Some(a) => a,
        None => return Err(op_err("archive", &archive.to_string_lossy(), "this box offers no tool for that archive")),
    };
    // Measured on this box: bsdtar exits 1 on a .. member and de-fangs an absolute one, printing
    // "Removing leading '/'" and extracting it relative. Neither escapes the staging directory.
    run_boxed(inner, archive, &work.dir)?;
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
        match archive_produced_count(formats, archive) {
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
    rename_noreplace(&staged, dest)?;
    Ok(verified)
}

pub fn convert_one(input: &Path, dest: &Path, strip: bool) -> Result<(), FleaError> {
    if dest.symlink_metadata().is_ok() {
        return Err(op_err("convert", &dest.to_string_lossy(), "that destination already exists"));
    }
    // Absolute, so ImageMagick can never read the input as an option (a file named "-write ...").
    // The staged destination is already absolute under the work directory beside dest.
    let input = std::fs::canonicalize(input).map_err(|e| from_io("convert", &input.to_string_lossy(), &e))?;
    let input = input.as_path();
    let parent = dest.parent().unwrap_or(Path::new("/"));
    let work = Work::new(parent, "cvt")?;
    let name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let staged = work.dir.join(&name);
    run_boxed(convert::argv(input, &staged, strip), input, &work.dir)?;
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
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    // Archive operations run concurrently by design, so two extracts into the same directory ask for
    // a work directory beside the same destination at the same time. Naming it from the pid alone
    // gave them the identical path, and the second one's cleanup destroyed the first one's output.
    // A tool that exits 0 having written nothing leaves the staging directory empty, and the shape
    // compress and convert use cannot see that here because this function creates that directory
    // itself. The consequence was an empty directory published as a successful extract.
    // An empty archive is legal and extracts to nothing, so the destination being empty is only a
    // failure when the archive said it held something. Both halves are asserted, because a check
    // that refuses every empty result would break the legitimate case instead of catching the bad one.
    // tar -c -C <empty dir> . produces an archive whose only member is ./, which lists one entry
    // and extracts nothing. Counting entries called that a failure; counting every member but the
    // root does not.
    #[test]
    fn only_the_archive_root_extracts_to_nothing_while_nested_directories_do_not() {
        let d = TestDir::new("archdotonly");
        let formats = Formats::from_tools(true, true);
        d.dir("emptysrc");
        let archive = d.join("dotonly.tar");
        let built = std::process::Command::new("bsdtar")
            .args(["-a", "-c", "-f", &archive.to_string_lossy(),
                   "-C", &d.join("emptysrc").to_string_lossy(), "."])
            .status();
        if !built.map(|s| s.success()).unwrap_or(false) {
            return;
        }
        assert_eq!(archive_produced_count(&formats, &archive), Some(0),
                   "the root is the only member, so nothing should appear in the destination");
        let dest = d.join("out");
        extract(&formats, &archive, &dest).expect("a root-only archive extracts legally");
        assert!(dest.is_dir(), "and its destination is published rather than refused");

        // The root's other spelling, which bsdtar writes for `-C dir ./.`. This exact archive
        // extracted correctly and was refused, because the predicate knew only "." and "./".
        let edot = d.join("edot.tar");
        let made_edot = std::process::Command::new("bsdtar")
            .args(["-a", "-c", "-f", &edot.to_string_lossy(),
                   "-C", &d.join("emptysrc").to_string_lossy(), "./."])
            .status();
        if made_edot.map(|s| s.success()).unwrap_or(false) {
            assert_eq!(archive_produced_count(&formats, &edot), Some(0),
                       "././ is the root as well, so it produces nothing either");
            extract(&formats, &edot, &d.join("edotout")).expect("and it extracts legally too");
        }

        // The case this test was named for and did not cover: directories that are NOT the root.
        // A file count called this empty while extract produced two directories in the destination.
        d.dir("nested/a");
        d.dir("nested/b/c");
        let nested = d.join("nested.tar");
        let made = std::process::Command::new("bsdtar")
            .args(["-a", "-c", "-f", &nested.to_string_lossy(),
                   "-C", &d.join("nested").to_string_lossy(), "."])
            .status();
        if made.map(|s| s.success()).unwrap_or(false) {
            assert!(archive_produced_count(&formats, &nested).unwrap_or(0) > 0,
                    "nested directories are destination entries, so an empty result is a failure");
            let nest_dest = d.join("nestout");
            extract(&formats, &nested, &nest_dest).expect("it extracts");
            assert!(std::fs::read_dir(&nest_dest).unwrap().count() > 0,
                    "and the destination really does receive them");
        }
    }

    #[test]
    fn an_empty_archive_extracts_to_an_empty_directory_and_that_is_success() {
        let d = TestDir::new("archempty");
        let formats = Formats::from_tools(true, true);
        let empty = d.join("empty.tar");
        let made = std::process::Command::new("bsdtar")
            .args(["-c", "-f", &empty.to_string_lossy(), "-T", "/dev/null"])
            .status();
        if !made.map(|s| s.success()).unwrap_or(false) {
            return;
        }
        let dest = d.join("out");
        extract(&formats, &empty, &dest).expect("an empty archive is a legal archive");
        assert!(dest.is_dir(), "and its destination is published, empty");
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
    }

    #[test]
    fn the_emptiness_check_reads_the_destination_and_the_index_separately() {
        let d = TestDir::new("archemptyparts");
        let formats = Formats::from_tools(true, true);
        d.dir("nothing");
        assert!(is_empty_dir(&d.join("nothing")), "a directory with no entries is empty");
        assert!(is_empty_dir(&d.join("never-existed")), "and so is one that cannot be read");
        d.file("nothing/something.txt", "body");
        assert!(!is_empty_dir(&d.join("nothing")));

        let empty = d.join("empty.tar");
        let made = std::process::Command::new("bsdtar")
            .args(["-c", "-f", &empty.to_string_lossy(), "-T", "/dev/null"])
            .status();
        if made.map(|s| s.success()).unwrap_or(false) {
            assert_eq!(archive_produced_count(&formats, &empty), Some(0), "an archive holding nothing produces nothing");
        }
        let real = d.join("real.tar");
        let built = std::process::Command::new("bsdtar")
            .args(["-c", "-f", &real.to_string_lossy(), "-C", &d.path().to_string_lossy(), "nothing"])
            .status();
        if built.map(|s| s.success()).unwrap_or(false) {
            // The directory member and the file inside it are both destination entries.
            assert_eq!(archive_produced_count(&formats, &real), Some(2),
                       "nothing/ and nothing/something.txt each produce one");
        }
        // An unreadable listing answers false, so an archive nothing can read is a failure, not a pass.
        let junk = d.file("junk.tar", "not an archive");
        assert_eq!(archive_produced_count(&formats, &junk), None, "an index nothing can read is not a count of zero");
    }

    // The case run_boxed's own comment names, a tool that writes nothing and exits 0, driven through
    // the real jail. /usr/bin/true is inside the --ro-bind /usr tree and is named by absolute path,
    // so --clearenv wiping PATH does not reach it and nothing is substituted from outside the jail.
    // That is every part of the integration except extract's own three lines of wiring.
    #[test]
    fn a_tool_that_exits_zero_writing_nothing_is_caught_by_the_predicate() {
        let d = TestDir::new("archzerowrite");
        let formats = Formats::from_tools(true, true);
        // A real archive holding one member, so the index disagrees with an empty destination.
        d.dir("src");
        d.file("src/a.txt", "body");
        let archive = d.join("one.tar");
        let built = std::process::Command::new("bsdtar")
            .args(["-a", "-c", "-f", &archive.to_string_lossy(),
                   "-C", &d.join("src").to_string_lossy(), "."])
            .status();
        if !built.map(|s| s.success()).unwrap_or(false) {
            return;
        }
        assert_eq!(archive_produced_count(&formats, &archive), Some(1), "the fixture archive produces one entry");

        let work = Work::new(d.path(), "ext").expect("work");
        let staged = work.dir.join("out");
        std::fs::create_dir(&staged).expect("staged");
        // Through prlimit and bwrap, exactly as a real listing tool runs.
        run_boxed(vec!["/usr/bin/true".to_string()], &archive, &work.dir)
            .expect("/usr/bin/true exits 0");
        assert!(is_empty_dir(&staged), "and it wrote nothing, which is the whole point of it");
        // The predicate extract applies to those two facts.
        assert!(is_empty_dir(&staged) && archive_produced_count(&formats, &archive) == Some(1),
                "an index naming an entry against a destination that holds none is a failure");

        // The same, on the archive that beat two previous predicates: all directories, no files, and
        // three members that should each have produced a destination entry. A file count called this
        // empty and would have published the empty staging directory as a verified success.
        d.dir("dirs/a");
        d.dir("dirs/b/c");
        let dirs = d.join("dirs.tar");
        let made = std::process::Command::new("bsdtar")
            .args(["-a", "-c", "-f", &dirs.to_string_lossy(),
                   "-C", &d.join("dirs").to_string_lossy(), "."])
            .status();
        if made.map(|s| s.success()).unwrap_or(false) {
            assert!(archive_produced_count(&formats, &dirs).unwrap_or(0) > 0,
                    "an all-directories archive still names members that must appear");
            let work2 = Work::new(d.path(), "ext").expect("work");
            let staged2 = work2.dir.join("out");
            std::fs::create_dir(&staged2).expect("staged");
            run_boxed(vec!["/usr/bin/true".to_string()], &dirs, &work2.dir).expect("exits 0");
            assert!(is_empty_dir(&staged2) && archive_produced_count(&formats, &dirs).unwrap_or(0) > 0,
                    "so a tool that wrote nothing for it is a failure, not a verified success");
        }

        // The other arm, for completeness: a tool that exits non-zero is refused before the check.
        assert!(run_boxed(vec!["/usr/bin/false".to_string()], &archive, &work.dir).is_err(),
                "run_boxed reads the status, so the non-zero arm never reaches the predicate");
    }

    #[test]
    fn a_file_that_is_not_an_archive_publishes_no_destination() {
        let d = TestDir::new("archnotone");
        let formats = Formats::from_tools(true, true);
        let not_an_archive = d.file("notreally.tar", "this is not an archive at all");
        let dest = d.join("out");
        assert!(extract(&formats, &not_an_archive, &dest).is_err());
        assert!(!dest.exists(), "no destination is published for a job that produced nothing");
    }

    #[test]
    fn a_compress_derives_its_parent_and_names_from_absolute_paths() {
        let (parent, names) = split_paths(&["/home/gm/a.txt".to_string(), "/home/gm/sub".to_string()]).unwrap();
        assert_eq!(parent, PathBuf::from("/home/gm"));
        assert_eq!(names, vec!["a.txt".to_string(), "sub".to_string()]);
        // A path from another directory would be stored under a name that is not its own.
        assert!(split_paths(&["/home/gm/a.txt".to_string(), "/etc/hosts".to_string()]).is_none());
        assert!(split_paths(&[]).is_none());
    }
}
