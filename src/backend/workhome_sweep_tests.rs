// RC-1: a stage a killed Flea left in the cache's work folder is cleared at the next backend start.
use super::*;
use crate::backend::testdir::TestDir;
use std::time::Duration;

const CASE: &str = "FLEA_WORKHOME_SWEEP_CASE";

// pid_max is one past the largest pid the kernel hands out, so no process ever runs under it.
fn never_running() -> u32 {
    let max = std::fs::read_to_string("/proc/sys/kernel/pid_max").unwrap();
    max.trim().parse().unwrap()
}

fn aged(path: &Path, age: Duration) {
    let at = SystemTime::now() - age;
    std::fs::File::open(path).unwrap().set_modified(at).unwrap();
}

fn stage(root: &Path, tag: &str, pid: u32, seq: usize, age: Duration) -> PathBuf {
    let path = root.join(format!("{WORK_PREFIX}{tag}-{pid}-{seq}"));
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("part.bin"), b"staged").unwrap();
    std::fs::create_dir(path.join("inner")).unwrap();
    aged(&path, age);
    path
}

const OLD: Duration = Duration::from_secs(2 * 24 * 60 * 60);

#[test]
fn a_dead_flea_s_day_old_stage_goes_and_nothing_else_does() {
    let d = TestDir::new("worksweep");
    let root = d.dir("work");
    let dead = never_running();
    let gone = stage(&root, "arc", dead, 1, OLD);
    let live = stage(&root, "ext", std::process::id(), 2, OLD);
    let young = stage(&root, "cvt", dead, 3, STALE_AFTER - Duration::from_secs(60));
    let foreign = root.join("notes");
    std::fs::create_dir(&foreign).unwrap();
    aged(&foreign, OLD);
    let unnamed = root.join(format!("{WORK_PREFIX}{dead}-4"));
    std::fs::create_dir(&unnamed).unwrap();
    aged(&unnamed, OLD);
    let file = root.join(format!("{WORK_PREFIX}arc-{dead}-5"));
    std::fs::write(&file, b"not a stage").unwrap();
    aged(&file, OLD);
    // A link carrying a stage's name leads out of the cache; it is not followed and its target stays whole.
    let outside = d.dir("outside");
    std::fs::write(outside.join("keep.txt"), b"keep").unwrap();
    aged(&outside, OLD);
    let link = root.join(format!("{WORK_PREFIX}arc-{dead}-6"));
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    assert_eq!(sweep(&root, SystemTime::now()), 1);
    assert!(!gone.exists(), "the killed Flea's day-old stage is cleared");
    for kept in [&live, &young, &foreign, &unnamed, &file] {
        assert!(kept.exists(), "{} stays", kept.display());
    }
    assert!(link.symlink_metadata().is_ok() && outside.join("keep.txt").is_file());
    assert_eq!(
        sweep(&root, SystemTime::now()),
        0,
        "a second start finds nothing more to clear"
    );
}

#[test]
fn stage_names_read_back_their_pid() {
    assert_eq!(stage_pid(".flea-work-ext-4123-7"), Some(4123));
    assert_eq!(stage_pid(".flea-work-convert-1-0"), Some(1));
    for name in [
        ".flea-work-4123-7",
        ".flea-work-ext-x-7",
        ".flea-work-ext-4123-",
        "flea-work-ext-4123-7",
        ".flea-work-",
    ] {
        assert_eq!(stage_pid(name), None, "{name}");
    }
}

// The start sweep reads XDG_CACHE_HOME, so it runs in a child test process pointed at a marked sandbox.
#[test]
fn the_backend_start_clears_the_cache_s_stale_stages() {
    if let Some(cache) = std::env::var_os(CASE) {
        let cache = PathBuf::from(cache);
        assert!(
            cache.is_absolute() && cache.parent().unwrap().join(".flea-test-sandbox").is_file()
        );
        assert_eq!(
            std::env::var_os("XDG_CACHE_HOME").as_deref(),
            Some(cache.as_os_str())
        );
        let handle = sweep_at_start().expect("the work folder exists, so the sweep runs");
        assert_eq!(handle.join().unwrap(), 1);
        return;
    }
    let d = TestDir::new("worksweep-start");
    let cache = d.dir("cache");
    let root = cache.join("flea").join("work");
    std::fs::create_dir_all(&root).unwrap();
    let gone = stage(&root, "arc", never_running(), 1, OLD);
    let live = stage(&root, "arc", std::process::id(), 2, OLD);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "backend::archivework::workhome::sweep_tests::the_backend_start_clears_the_cache_s_stale_stages", "--nocapture"])
        .env(CASE, &cache)
        .env("XDG_CACHE_HOME", &cache)
        .output()
        .unwrap();
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(
        child.status.success() && said.contains("running 1 test"),
        "{said}"
    );
    assert!(!gone.exists() && live.exists(), "only the stale stage went");
}
