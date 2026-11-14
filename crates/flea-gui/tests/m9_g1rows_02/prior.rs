//! A marked fake provider keeps the deliberate later child observable without changing the backend.
use super::helpers;
use super::process::{build_dir, fixture, marked_root, rustc, DEADLINE};
use super::{first_listing, startup, HEIGHT, SMALL};
use flea_gui::list::pane::Pane;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// Longer than the product's one-second GRACE, so every sample sees the delayed prior start.
const PRIOR_SETTLE: Duration = Duration::from_secs(2);

const PROVIDER: &str = r#"
use std::{env, fs, io::Write, process::{Command, Stdio}, thread, time::{Duration, Instant}};
const HOLD: Duration = Duration::from_secs(120);
fn main() {
    let root = std::path::PathBuf::from(env::var_os("FLEA_FAKE_ROOT").unwrap());
    let args: Vec<_> = env::args().collect();
    if args[0] == "zoxide" { assert_eq!(&args[1..], ["query", "--list", "--all", "--score"]); }
    let mut pids = fs::OpenOptions::new().create(true).append(true).open(root.join("provider.pids")).unwrap();
    writeln!(pids, "{}", std::process::id()).unwrap();
    let mut extra = match env::var("FLEA_FAKE_MODE").as_deref() {
        Ok("extra") => Some(Command::new("unexpected-helper").arg("surprise").env_remove("FLEA_FAKE_MODE")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()),
        Ok("duplicate") => Some(Command::new("zoxide").args(["query", "--list", "--all", "--score"]).env_remove("FLEA_FAKE_MODE")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()),
        _ => None,
    };
    let until = Instant::now() + HOLD;
    while !root.join("provider.release").exists() && Instant::now() < until { thread::sleep(Duration::from_millis(5)); }
    if let Some(child) = extra.as_mut() { let _ = child.wait(); }
    println!("1 {}", root.display());
}
"#;

fn binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let source = build_dir().join("prior-provider.rs");
        let binary = build_dir().join("prior-provider");
        fs::write(&source, PROVIDER).unwrap();
        rustc(&[
            "--edition=2021".as_ref(),
            "--crate-name=prior_provider".as_ref(),
            source.as_os_str(),
            "-o".as_ref(),
            binary.as_os_str(),
        ]);
        binary
    })
}

struct Fake<'a> {
    root: &'a Path,
    mode: &'a str,
}

impl<'a> Fake<'a> {
    fn new(root: &'a Path, mode: &'a str) -> Self {
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        for name in ["zoxide", "unexpected-helper"] {
            let path = bin.join(name);
            fs::copy(binary(), &path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { root, mode }
    }

    fn configure(&self, command: &mut Command) {
        command
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("FLEA_FAKE_ROOT", self.root)
            .env("FLEA_FAKE_MODE", self.mode);
    }

    fn pids(&self) -> Vec<u32> {
        fs::read_to_string(self.root.join("provider.pids"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse().ok())
            .collect()
    }

    fn ready(&self) {
        let count = if self.mode == "good" { 1 } else { 2 };
        let deadline = Instant::now() + DEADLINE;
        while self.pids().len() < count {
            assert!(Instant::now() < deadline, "the fake provider starts");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn release(&self) {
        fs::write(self.root.join("provider.release"), b"").unwrap();
        let deadline = Instant::now() + DEADLINE;
        // Sample input: "4321 (zoxide) S 4300 ..." in /proc/<pid>/stat; the state follows the last ')'.
        while self.pids().iter().any(|pid| {
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            !stat.is_empty() && !stat[stat.rfind(')').unwrap() + 2..].starts_with('Z')
        }) {
            assert!(Instant::now() < deadline, "the fake providers leave");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Fake<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

fn observe(mode: &str) -> startup::Startup {
    let root = marked_root("late-prior");
    let fake = Fake::new(&root, mode);
    let dir = fixture(&root, SMALL);
    let pane = Pane::new(dir.to_string_lossy().into_owned(), HEIGHT);
    let found = startup::sample(
        &root,
        &dir,
        &first_listing(&pane, &dir),
        PRIOR_SETTLE,
        |command| fake.configure(command),
        || fake.ready(),
        || fake.release(),
    );
    println!(
        "M9_G1ROWS_02 late mode={mode} spawns={} children={} threads={} argv={}",
        found.spawns, found.children, found.threads, found.spawn_argv
    );
    found
}

#[test]
fn m9_g1rows_02_late_prior_is_not_startup_work() {
    let found = observe("good");
    assert_eq!(found.spawns, 1, "the fake prior actually ran");
    assert_eq!(
        found.children, 1,
        "the fake prior stays alive during sampling"
    );
    assert_eq!(
        found.child_argv,
        [helpers::PRIOR_ARGV],
        "the live child is the prior"
    );
    found.assert_helpers("late prior");
}

fn rejected(found: &startup::Startup) -> String {
    let error = std::panic::catch_unwind(|| found.assert_helpers("negative control"))
        .expect_err("extra helper work must fail");
    error
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| error.downcast_ref::<&str>().map(|text| (*text).to_owned()))
        .expect("the assertion explains the failure")
}

#[test]
fn m9_g1rows_02_another_helper_still_fails_with_its_argv() {
    let found = observe("extra");
    let error = rejected(&found);
    assert!(
        error.contains("unexpected helper argv") && error.contains("unexpected-helper"),
        "{error}"
    );
}

#[test]
fn m9_g1rows_02_a_second_prior_helper_still_fails() {
    let found = observe("duplicate");
    let error = rejected(&found);
    assert!(
        error.contains("more than one helper") && error.contains("zoxide"),
        "{error}"
    );
}

#[test]
fn an_extra_or_foreign_child_fails_the_helper_budget() {
    let record = format!("posix_spawnp path/argv: \"zoxide\" {}", helpers::PRIOR_ARGV);
    let extra = startup::Startup {
        spawns: 1,
        children: 2,
        child_argv: vec![helpers::PRIOR_ARGV.into(), helpers::PRIOR_ARGV.into()],
        spawn_argv: record.clone(),
        ..Default::default()
    };
    assert!(rejected(&extra).contains("no other child is allowed"));
    let foreign = startup::Startup {
        spawns: 1,
        children: 1,
        child_argv: vec![r#""unexpected-helper" "surprise""#.into()],
        spawn_argv: record,
        ..Default::default()
    };
    assert!(rejected(&foreign).contains("unexpected child \"unexpected-helper\""));
}
