// Review-only probes for g2-st2 round 4: what a record or config read takes from disk, and a clone's nested dot entries.
use super::plugins_tests::{ask, code, env, plugin, READY};
use super::testdir::TestDir;
use std::path::Path;
use std::process::{Command, Stdio};

// Bytes this thread has read through read(2) so far, from the kernel's own per-thread counter.
fn rchar() -> u64 {
    let io = std::fs::read_to_string("/proc/thread-self/io").expect("per-thread io counters");
    io.lines()
        .find_map(|l| l.strip_prefix("rchar: "))
        .and_then(|v| v.trim().parse().ok())
        .expect("rchar line")
}

// Runs one read on a fresh thread and answers its result with the bytes that thread read.
fn counted<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> (T, u64) {
    super::fifotest::within("a counted read", move || {
        let before = rchar();
        let out = call();
        (out, rchar() - before)
    })
}

fn sparse(path: &Path, len: u64) {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .unwrap()
        .set_len(len)
        .unwrap();
}

#[test]
fn rev4_st2_a_record_read_takes_at_most_its_limit_from_disk() {
    let dir = TestDir::new("rev4-st2-recbytes");
    let store = dir.dir("config/flea/plugins");
    dir.dir("config/flea/plugins/.sources");
    let record = dir.join("config/flea/plugins/.sources/me.tool");
    dir.assert_contains(&record);
    sparse(&record, 16 << 20);
    let (source, read) = counted(move || super::pluginfs::read_source(&store, "me.tool"));
    let limit = super::pluginfs::SOURCE_LIMIT;
    eprintln!("REV4 read_source over a 16 MiB record: {source:?}, {read} bytes read");
    assert_eq!(source, None);
    assert!(
        read <= limit + 1 + 1024,
        "read_source took {read} bytes from disk for a {limit} byte limit"
    );
}

#[test]
fn rev4_st2_a_config_read_takes_at_most_its_limit_from_disk() {
    let dir = TestDir::new("rev4-st2-cfgbytes");
    let folder = dir.dir("config/flea/plugins/me.dev");
    dir.dir("config/flea/plugins/me.dev/.git");
    let config = dir.join("config/flea/plugins/me.dev/.git/config");
    dir.assert_contains(&config);
    sparse(&config, 16 << 20);
    let (origin, read) = counted(move || super::plugingit::git_origin(&folder));
    eprintln!("REV4 git_origin over a 16 MiB .git/config: {origin:?}, {read} bytes read");
    assert_eq!(origin, None);
    assert!(
        read <= (64 << 10) + 1 + 1024,
        "git_origin took {read} bytes from disk for a 64 KiB limit"
    );
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?}");
}

fn clone_add(dir: &TestDir, id: &str, extra: impl FnOnce(&TestDir)) -> (&'static str, bool) {
    let env = env(dir);
    let up = plugin(dir, "up", id, READY);
    extra(dir);
    git(&up, &["init", "--quiet", "--initial-branch=main"]);
    git(&up, &["add", "-A"]);
    git(&up, &["commit", "--quiet", "-m", "one"]);
    let line = format!(
        r#"{{"c":"plugins","op":"add","id":1,"source":"file://{}"}}"#,
        up.display()
    );
    let refused = code(ask(&env, &line).0);
    (refused, env.dir.join(id).exists())
}

#[test]
fn rev4_st2_a_git_add_past_the_entry_cap_in_a_nested_dot_folder_never_lands() {
    let dir = TestDir::new("rev4-st2-nestmany");
    let (refused, landed) = clone_add(&dir, "me.many", |d| {
        d.dir("up/lib/.cache/.deep");
        for n in 0..4200 {
            d.file(&format!("up/lib/.cache/.deep/{n}"), "");
        }
    });
    eprintln!(
        "REV4 git add with 4200 files under lib/.cache/.deep: code {refused:?}, landed {landed}"
    );
    assert_eq!(refused, "write");
    assert!(!landed);
}

#[test]
fn rev4_st2_a_git_add_past_the_byte_cap_in_a_nested_dot_file_never_lands() {
    let dir = TestDir::new("rev4-st2-nestbig");
    let (refused, landed) = clone_add(&dir, "me.big", |d| {
        d.dir("up/lib/.x");
        sparse(&d.join("up/lib/.x/.blob"), 65 << 20);
    });
    eprintln!("REV4 git add with a 65 MiB lib/.x/.blob: code {refused:?}, landed {landed}");
    assert_eq!(refused, "write");
    assert!(!landed);
}

#[test]
fn rev4_st2_control_a_small_clone_with_dot_entries_lands() {
    let dir = TestDir::new("rev4-st2-dotok");
    let (refused, landed) = clone_add(&dir, "me.ok", |d| {
        d.dir("up/.github/workflows");
        d.file("up/.github/workflows/ci.yml", "on: push\n");
        d.file("up/.gitignore", "target\n");
    });
    eprintln!("REV4 git add with small dot entries: code {refused:?}, landed {landed}");
    assert_eq!(refused, "");
    assert!(landed);
}
