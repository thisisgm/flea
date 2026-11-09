// What an Update may never do: read a folder's .git, obey a repository's own config, or run its hooks.
use super::plugins::Env;
use super::plugins_tests::{ask, code, env, get, plugin, READY};
use super::testdir::TestDir;
use flea::ui_logic_plugin_settings::wire::Source;
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

// An upstream repository with one commit of a plugin at the given version.
fn upstream(dir: &TestDir, version: &str) -> PathBuf {
    let repo = plugin(dir, "up", "me.dev", READY);
    let manifest = repo.join("manifest.json");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("\"1.0.0\"", &format!("\"{version}\""));
    dir.replace_file(&manifest, &text);
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "one"]);
    repo
}

// A developer's working checkout: a clone of upstream that the user adds as a local folder.
fn checkout(dir: &TestDir) -> (PathBuf, PathBuf) {
    let up = upstream(dir, "1.0.0");
    let work = dir.join("work");
    let status = Command::new("git")
        .args(["clone", "--quiet", "--"])
        .arg(&up)
        .arg(&work)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    (up, work)
}

fn upstream_adds(dir: &TestDir, up: &Path, name: &str) {
    dir.file(&format!("up/{name}"), "new\n");
    git(up, &["add", "-A"]);
    git(up, &["commit", "--quiet", "-m", "two"]);
}

fn add(env: &Env, source: &str) -> &'static str {
    code(
        ask(
            env,
            &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{source}"}}"#),
        )
        .0,
    )
}

#[test]
fn rev_st2_a_folder_add_of_a_checkout_is_a_folder_plugin_without_update() {
    let dir = TestDir::new("rev-st2-src");
    let env = env(&dir);
    let (up, work) = checkout(&dir);
    assert_eq!(add(&env, &work.display().to_string()), "");
    assert!(
        !env.dir.join("me.dev/.git").exists(),
        "a folder add never carries the folder's .git"
    );
    upstream_adds(&dir, &up, "later.txt");
    let (done, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    assert_eq!(
        get(&listing, "me.dev").source,
        Source::Folder(work.display().to_string()),
        "added from a folder, so SOURCE names the folder"
    );
    assert_eq!(
        code(done),
        "git",
        "docs/protocol.md: update reaches only a plugin added from Git"
    );
    assert!(!env.dir.join("me.dev/later.txt").exists());
}

#[test]
fn rev_st2_update_never_writes_outside_the_plugin_store() {
    let dir = TestDir::new("rev-st2-wt");
    let env = env(&dir);
    let (up, work) = checkout(&dir);
    let outside = dir.dir("outside");
    git(
        &work,
        &["config", "core.worktree", &outside.display().to_string()],
    );
    add(&env, &work.display().to_string());
    upstream_adds(&dir, &up, "pwned.txt");
    ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    assert!(
        !outside.join("pwned.txt").exists(),
        "Update wrote {} outside the plugins folder",
        outside.join("pwned.txt").display()
    );
}

#[test]
fn rev_st2_update_of_a_disabled_plugin_runs_none_of_its_code() {
    let dir = TestDir::new("rev-st2-hook");
    let env = env(&dir);
    let (up, _) = checkout(&dir);
    let marker = dir.join("hook-ran");
    dir.script(
        "work/.git/hooks/post-merge",
        &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    );
    add(&env, &dir.join("work").display().to_string());
    upstream_adds(&dir, &up, "two.txt");
    let (_, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    assert!(!get(&listing, "me.dev").enabled);
    assert!(
        !marker.exists(),
        "Update ran a hook the added folder carried while the plugin is off"
    );
}

#[test]
fn st2_fix_update_refuses_a_git_plugin_whose_config_turns_hostile() {
    for key in ["core.worktree", "core.fsmonitor", "core.hooksPath"] {
        let dir = TestDir::new("st2-hostile");
        let env = env(&dir);
        let up = upstream(&dir, "1.0.0");
        assert_eq!(add(&env, &format!("file://{}", up.display())), "", "{key}");
        let installed = env.dir.join("me.dev");
        let marker = dir.join("ran");
        let touch = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
        dir.script("config/flea/plugins/me.dev/.git/hooks/post-merge", &touch);
        let hooks = dir.dir("hooks");
        dir.script("hooks/post-merge", &touch);
        let monitor = dir.script("monitor", &touch);
        let outside = dir.dir("outside");
        let value = match key {
            "core.worktree" => outside.clone(),
            "core.fsmonitor" => monitor,
            _ => hooks,
        };
        git(&installed, &["config", key, &value.display().to_string()]);
        upstream_adds(&dir, &up, "later.txt");
        let (done, _) = ask(
            &env,
            r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
        );
        assert_eq!(code(done), "git", "{key}: Update refuses the repository");
        assert!(
            !marker.exists(),
            "{key}: Update ran code the repository named"
        );
        assert!(
            !installed.join("later.txt").exists() && !outside.join("later.txt").exists(),
            "{key}: nothing was pulled"
        );
    }
}

#[test]
fn st2_fix_update_of_a_current_git_plugin_answers_its_version() {
    let dir = TestDir::new("st2-current");
    let env = env(&dir);
    let up = upstream(&dir, "0.2.0");
    assert_eq!(add(&env, &format!("file://{}", up.display())), "");
    let (done, _) = ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    let done = done.unwrap();
    assert_eq!(
        (done.error, done.version.as_str()),
        (None, "0.2.0"),
        "the board's Up to date state"
    );
}

// NATIVE-RUN.md's plugin-git-page fixture: the board's https origin on SOURCE, the pull served from a local bare repo.
#[test]
fn st2_fix_the_git_page_fixture_reaches_up_to_date_offline() {
    let dir = TestDir::new("st2-scene");
    let env = env(&dir);
    let up = upstream(&dir, "0.2.0");
    let bare = dir.join("origin.git");
    let status = Command::new("git")
        .args(["clone", "--quiet", "--bare", "--"])
        .arg(&up)
        .arg(&bare)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(add(&env, &format!("file://{}", bare.display())), "");
    let installed = env.dir.join("me.dev");
    let head = Command::new("git")
        .arg("-C")
        .arg(&installed)
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .unwrap();
    let branch = String::from_utf8(head.stdout).unwrap().trim().to_string();
    let board = "https://github.com/mtorvik/flea-checksums";
    git(&installed, &["config", "remote.origin.url", board]);
    git(
        &installed,
        &[
            "config",
            &format!("branch.{branch}.remote"),
            &bare.display().to_string(),
        ],
    );
    let (done, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    let done = done.unwrap();
    assert_eq!((done.error, done.version.as_str()), (None, "0.2.0"));
    assert_eq!(get(&listing, "me.dev").source, Source::Git(board.into()));
}
