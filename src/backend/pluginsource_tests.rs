// Where a plugin's source is kept and read, Git with its hooks off, and a clone held to the folder add's bounds.
use super::fifotest::{mkfifo, within};
use super::plugins_tests::{ask, code, env, get, plugin, READY};
use super::testdir::TestDir;
use flea::ui_logic_plugin_settings::wire::Source;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", message]);
}

fn add_folder(dir: &TestDir) -> (super::plugins::Env, PathBuf) {
    let env = env(dir);
    let src = plugin(dir, "src", "me.tool", READY);
    let line = format!(
        r#"{{"c":"plugins","op":"add","id":1,"source":"{}"}}"#,
        src.display()
    );
    assert_eq!(code(ask(&env, &line).0), "");
    (env, src)
}

// The record of a folder add, one small file per plugin in the store's own dot folder.
fn sidecar(env: &super::plugins::Env) -> PathBuf {
    env.dir.join(".sources/me.tool")
}

fn listed(env: &super::plugins::Env) -> Source {
    get(
        &ask(env, r#"{"c":"plugins","op":"list","id":9}"#).1,
        "me.tool",
    )
    .source
    .clone()
}

#[test]
fn st2_fix2_a_folder_add_keeps_its_source_outside_the_plugin_folder() {
    let dir = TestDir::new("st2-sidecar");
    let (env, src) = add_folder(&dir);
    assert_eq!(listed(&env), Source::Folder(src.display().to_string()));
    assert!(
        sidecar(&env).is_file(),
        "the add wrote its record beside the plugins, not inside one"
    );
    assert!(
        !env.dir.join("me.tool/.flea-source").exists(),
        "nothing of the record is inside the plugin"
    );
    dir.file(
        "config/flea/plugins/me.tool/.flea-source",
        "/home/me/elsewhere\n",
    );
    assert_eq!(
        listed(&env),
        Source::Folder(src.display().to_string()),
        "a file inside the plugin decides nothing"
    );
    let (done, _) = ask(
        &env,
        r#"{"c":"plugins","op":"remove","id":2,"plugin":"me.tool"}"#,
    );
    assert_eq!(code(done), "");
    assert!(
        !sidecar(&env).exists(),
        "remove deletes the record with the plugin"
    );
}

#[test]
fn st2_fix2_a_symlinked_sidecar_is_refused() {
    let dir = TestDir::new("st2-sidecar-link");
    let (env, _) = add_folder(&dir);
    let elsewhere = dir.file("elsewhere", "/home/me/elsewhere\n");
    let record = sidecar(&env);
    let _ = std::fs::remove_file({
        dir.assert_contains(&record);
        &record
    });
    symlink(&elsewhere, &record).unwrap();
    assert_eq!(listed(&env), Source::Unknown);
}

#[test]
fn st2_fix2_a_fifo_sidecar_is_refused_without_blocking() {
    let dir = TestDir::new("st2-sidecar-fifo");
    let (env, _) = add_folder(&dir);
    let record = sidecar(&env);
    let _ = std::fs::remove_file({
        dir.assert_contains(&record);
        &record
    });
    mkfifo(&record);
    let source = within("a listing over a fifo sidecar", move || listed(&env));
    assert_eq!(source, Source::Unknown);
}

#[test]
fn st2_fix2_an_oversized_sidecar_is_refused_within_the_limit() {
    let dir = TestDir::new("st2-sidecar-big");
    let (env, _) = add_folder(&dir);
    let record = sidecar(&env);
    // Sparse past 4 KiB and under the suite's file size limit; st2_fix3_a_record_read_stops_at_its_limit pins the bound.
    std::fs::OpenOptions::new()
        .write(true)
        .open({
            dir.assert_contains(&record);
            &record
        })
        .unwrap()
        .set_len(16 << 20)
        .unwrap();
    let source = within("a listing over a 16 MiB record", move || listed(&env));
    assert_eq!(source, Source::Unknown);
}

#[test]
fn st2_fix2_update_runs_no_hook_of_a_clean_git_plugin() {
    let dir = TestDir::new("st2-hooks");
    let env = env(&dir);
    let up = plugin(&dir, "up", "me.dev", READY);
    git(&up, &["init", "--quiet"]);
    commit_all(&up, "one");
    let line = format!(
        r#"{{"c":"plugins","op":"add","id":1,"source":"file://{}"}}"#,
        up.display()
    );
    assert_eq!(code(ask(&env, &line).0), "");
    let marker = dir.join("hook-ran");
    let touch = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
    for hook in [
        "post-merge",
        "post-checkout",
        "pre-merge-commit",
        "reference-transaction",
    ] {
        dir.script(
            &format!("config/flea/plugins/me.dev/.git/hooks/{hook}"),
            &touch,
        );
    }
    dir.file("up/later.txt", "new\n");
    commit_all(&up, "two");
    let (done, _) = ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    assert_eq!(code(done), "", "a clean repository updates");
    assert!(env.dir.join("me.dev/later.txt").exists(), "the pull ran");
    assert!(!marker.exists(), "Git ran a hook the repository carried");
}

#[test]
fn st2_fix2_a_git_add_past_the_entry_cap_never_lands() {
    let dir = TestDir::new("st2-git-cap");
    let env = env(&dir);
    let up = plugin(&dir, "up", "me.many", READY);
    dir.dir("up/many");
    for n in 0..4096 {
        dir.file(&format!("up/many/{n}"), "");
    }
    git(&up, &["init", "--quiet"]);
    commit_all(&up, "one");
    let line = format!(
        r#"{{"c":"plugins","op":"add","id":1,"source":"file://{}"}}"#,
        up.display()
    );
    let (done, listing) = ask(&env, &line);
    assert_eq!(code(done), "write");
    assert!(
        listing.plugins.iter().all(|p| p.manifest.id != "me.many"),
        "nothing landed in the store"
    );
    assert!(!env.dir.join("me.many").exists());
}

#[test]
fn st2_fix2_git_runs_with_fsmonitor_off() {
    let dir = TestDir::new("st2-fsmonitor");
    let repo = plugin(&dir, "repo", "me.dev", READY);
    git(&repo, &["init", "--quiet"]);
    commit_all(&repo, "one");
    let marker = dir.join("monitor-ran");
    let monitor = dir.script(
        "monitor",
        &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    );
    git(
        &repo,
        &["config", "core.fsmonitor", &monitor.display().to_string()],
    );
    let at = repo.display().to_string();
    let ran = super::plugingit::git(
        &["-C", &at, "status", "--porcelain"],
        std::time::Duration::from_secs(30),
    );
    assert!(ran.is_ok(), "{ran:?}");
    assert!(!marker.exists(), "Git ran the repository's fsmonitor");
}

#[test]
fn st2_fix2_a_member_swapped_for_a_fifo_or_a_link_is_refused_without_blocking() {
    let dir = TestDir::new("st2-member");
    let fifo = dir.join("pipe");
    mkfifo(&fifo);
    let refused = within("an open of a fifo member", move || {
        super::pluginfs::open_member(&fifo).is_err()
    });
    assert!(refused, "a fifo is not a plugin file");
    let target = dir.file("target", "x");
    let link = dir.join("link");
    symlink(&target, &link).unwrap();
    assert!(
        super::pluginfs::open_member(&link).is_err(),
        "a link is not followed"
    );
    assert!(super::pluginfs::open_member(&target).is_ok());
}

// Serves a bytes up to a ceiling far past any limit and counts what it served.
struct Endless {
    served: u64,
}

impl std::io::Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min((64 << 20) - self.served as usize);
        buf[..n].fill(b'a');
        self.served += n as u64;
        Ok(n)
    }
}

#[test]
fn st2_fix3_a_record_read_stops_at_its_limit() {
    let mut from = Endless { served: 0 };
    let limit = super::pluginfs::SOURCE_LIMIT;
    assert_eq!(
        super::pluginfs::read_bounded(&mut from, limit),
        None,
        "a record past the limit is refused"
    );
    assert!(
        from.served <= limit + 1,
        "the read took {} bytes for a {limit} byte limit",
        from.served
    );
    let mut short = Endless {
        served: (64 << 20) - 10,
    };
    assert_eq!(
        super::pluginfs::read_bounded(&mut short, limit).as_deref(),
        Some("aaaaaaaaaa")
    );
}
