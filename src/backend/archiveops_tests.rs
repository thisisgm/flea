use super::*;
use crate::backend::archivework::archive_produced_count;
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
    if crate::backend::sandboxprobe::skipped() { return; }
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
    extract(&formats, &archive, &dest, &AtomicBool::new(false)).expect("a root-only archive extracts legally");
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
        extract(&formats, &edot, &d.join("edotout"), &AtomicBool::new(false)).expect("and it extracts legally too");
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
        extract(&formats, &nested, &nest_dest, &AtomicBool::new(false)).expect("it extracts");
        assert!(std::fs::read_dir(&nest_dest).unwrap().count() > 0,
                "and the destination really does receive them");
    }
}

#[test]
fn an_empty_archive_extracts_to_an_empty_directory_and_that_is_success() {
    if crate::backend::sandboxprobe::skipped() { return; }
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
    extract(&formats, &empty, &dest, &AtomicBool::new(false)).expect("an empty archive is a legal archive");
    assert!(dest.is_dir(), "and its destination is published, empty");
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
}

#[test]
fn the_emptiness_check_reads_the_destination_and_the_index_separately() {
    if crate::backend::sandboxprobe::skipped() { return; }
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

// A tool that writes nothing and exits 0, driven through the real jail. /usr/bin/true is inside
// the real jail. /usr/bin/true is inside the --ro-bind /usr tree and is named by absolute path,
// so --clearenv wiping PATH does not reach it and nothing is substituted from outside the jail.
// That is every part of the integration except extract's own three lines of wiring.
#[test]
fn a_tool_that_exits_zero_writing_nothing_is_caught_by_the_predicate() {
    if crate::backend::sandboxprobe::skipped() { return; }
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

    let mut work = Work::new(d.path(), "ext").expect("work");
    let staged = work.dir.join("out");
    std::fs::create_dir(&staged).expect("staged");
    let idle = AtomicBool::new(false);
    // Through prlimit and bwrap, exactly as a real listing tool runs.
    run_boxed_cancellable("archive", vec!["/usr/bin/true".to_string()], &archive, &mut work, &idle)
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
        let mut work2 = Work::new(d.path(), "ext").expect("work");
        let staged2 = work2.dir.join("out");
        std::fs::create_dir(&staged2).expect("staged");
        run_boxed_cancellable("archive", vec!["/usr/bin/true".to_string()], &dirs, &mut work2, &idle).expect("exits 0");
        assert!(is_empty_dir(&staged2) && archive_produced_count(&formats, &dirs).unwrap_or(0) > 0,
                "so a tool that wrote nothing for it is a failure, not a verified success");
    }

    // The other arm, for completeness: a tool that exits non-zero is refused before the check.
    assert!(run_boxed_cancellable("archive", vec!["/usr/bin/false".to_string()], &archive, &mut work, &idle).is_err(),
            "the runner reads the status, so the non-zero arm never reaches the predicate");
}

// GM converted an image and was told the archive tool had failed: one jail runs both, and every
// failure out of it named "archive". A tool that exits non-zero with nothing on stderr is the
// only case that reaches this wording, which is why it went unnoticed for so long.
#[test]
fn a_silent_failure_names_the_operation_that_was_running() {
    if crate::backend::sandboxprobe::skipped() { return; }
    let d = TestDir::new("archwho");
    let mut work = Work::new(d.path(), "who").expect("work directory");
    let quiet = vec!["/usr/bin/false".to_string()];
    let idle = AtomicBool::new(false);
    let converting = run_boxed_cancellable_capped("convert", quiet.clone(), d.path(), &mut work, &idle).unwrap_err();
    assert_eq!(converting.where_, "convert", "an image conversion says so");
    assert!(converting.msg.contains("convert") && !converting.msg.contains("archive"),
            "and its wording does too: {}", converting.msg);
    let archiving = run_boxed_cancellable("archive", quiet, d.path(), &mut work, &idle).unwrap_err();
    assert_eq!(archiving.where_, "archive", "and an archive still says archive");
}

#[test]
fn a_file_that_is_not_an_archive_publishes_no_destination() {
    let d = TestDir::new("archnotone");
    let formats = Formats::from_tools(true, true);
    let not_an_archive = d.file("notreally.tar", "this is not an archive at all");
    let dest = d.join("out");
    assert!(extract(&formats, &not_an_archive, &dest, &AtomicBool::new(false)).is_err());
    assert!(!dest.exists(), "no destination is published for a job that produced nothing");
}

// #156: the fixture is built by the HOST's bsdtar under a pinned locale; the jail extract is asserted.
#[test]
fn a_utf8_member_name_round_trips_through_the_jail_byte_for_byte() {
    if crate::backend::sandboxprobe::skipped() { return; }
    let d = TestDir::new("archutf8");
    let formats = Formats::from_tools(true, true);
    let folder = "Remoção de Viés 1º Mês";
    let member = "Relatório Março.txt";
    let source = d.join(folder);
    std::fs::create_dir_all(&source).expect("source directory");
    std::fs::write(source.join(member), b"conteudo\n").expect("payload");
    let archive = d.join("bundle.tar");
    let mut build = std::process::Command::new("bsdtar");
    build.env("LC_ALL", "C.UTF-8")
        .args(["-a", "-c", "-f", &archive.to_string_lossy(),
               "-C", &d.path().to_string_lossy(), folder]);
    let built = match build.status() {
        Ok(status) => status,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Missing bsdtar is a missing prerequisite, so it skips; nothing else is a skip.
            std::io::Write::write_all(&mut std::io::stderr(), b"SKIP backend::archiveops::tests::a_utf8_member_name_round_trips_through_the_jail_byte_for_byte: no bsdtar on this box, so the fixture cannot be built\n").ok();
            return;
        }
        Err(e) => panic!("bsdtar could not be started: {}", e),
    };
    assert!(built.success(), "bsdtar failed to build the fixture archive");
    let dest = d.join("out");
    extract(&formats, &archive, &dest, &AtomicBool::new(false)).expect("a utf8 name extracts through the real jail");
    assert!(dest.join(folder).is_dir(), "the folder name survived the jail");
    assert_eq!(std::fs::read(dest.join(folder).join(member)).unwrap(), b"conteudo\n",
               "the member name and its bytes survived the jail");
}

// A cancel before any child runs publishes nothing and leaves no staging directory.
#[test]
fn a_cancelled_extract_publishes_nothing_and_leaves_no_staging_directory() {
    use crate::backend::archivework::WORK_PREFIX;
    let d = TestDir::new("archcancelextract");
    let formats = Formats::from_tools(true, true);
    let dest = d.join("out");
    let e = extract(&formats, Path::new("/nonexistent/a.zip"), &dest, &AtomicBool::new(true)).unwrap_err();
    assert_eq!(e.msg, "cancelled");
    assert!(!dest.exists(), "a cancelled extract published a destination");
    let litter = std::fs::read_dir(d.path()).unwrap()
        .filter(|entry| entry.as_ref().map(|e| e.file_name().to_string_lossy().starts_with(WORK_PREFIX)).unwrap_or(false))
        .count();
    assert_eq!(litter, 0, "a cancelled extract left its staging directory behind");
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

// A user format's program lives outside /usr, so the jail binds that one file and nothing beside it:
// the tool runs, receives every selected name as its own argument, and cannot see its own directory.
#[test]
fn a_user_format_runs_its_own_program_in_the_jail_and_sees_nothing_else_of_its_directory() {
    if crate::backend::sandboxprobe::skipped() { return; }
    let d = TestDir::new("archuser");
    d.dir("src");
    d.file("src/a b.txt", "a");
    d.file("src/-dash", "b");
    d.dir("bin");
    d.file("bin/secret", "not for the jail");
    let tool = d.script("bin/tool", "#!/bin/sh\nout=$1; dir=$2; shift 3\n\
        if [ -e \"$(dirname \"$0\")/secret\" ]; then seen=seen; else seen=hidden; fi\n\
        { echo \"$seen\"; for n in \"$@\"; do [ -f \"$dir/$n\" ] && echo \"$n\"; done; } > \"$out\"\n");
    let line = format!("mine {} {{out}} {{dir}} -- {{names}}", tool.display());
    let (user, refused) = crate::backend::archiveuser::parse(&line, &[], |p| Some(p.to_path_buf()));
    assert!(refused.is_empty(), "{:?}", refused);
    let formats = Formats::from_tools(true, false).with_user(user);
    assert!(formats.offers("mine"));
    let dest = d.join("src.mine");
    let names = ["a b.txt".to_string(), "-dash".to_string()];
    compress(&formats, &d.join("src"), &names, "mine", &dest, &AtomicBool::new(false)).expect("the user tool compresses");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hidden\na b.txt\n-dash\n");
}
