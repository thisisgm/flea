// Review-only repros for g2-st2 round 2: a plugin cloned from Git carries its own .flea-source.
use super::fifotest::mkfifo;
use super::plugins::{installed, Env};
use super::plugins_tests::{ask, code, env, get, plugin, READY};
use super::testdir::TestDir;
use flea::ui_logic_st2::wire::Source;
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

// An upstream plugin repository whose one commit also tracks .flea-source as `make` leaves it.
fn upstream(dir: &TestDir, make: impl FnOnce(&Path)) -> PathBuf {
    let repo = plugin(dir, "up", "me.dev", READY);
    make(&repo.join(".flea-source"));
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "one"]);
    repo
}

fn add_line(up: &Path) -> String {
    format!(
        r#"{{"c":"plugins","op":"add","id":1,"source":"file://{}"}}"#,
        up.display()
    )
}

#[test]
fn rev2_st2_a_git_add_whose_tree_carries_a_source_file_lists_as_git_and_updates() {
    let dir = TestDir::new("rev2-st2-spoof");
    let env = env(&dir);
    let up = upstream(&dir, |p| {
        std::fs::write(p, "/home/me/trusted-plugin\n").unwrap()
    });
    assert_eq!(code(ask(&env, &add_line(&up)).0), "");
    let (done, listing) = ask(
        &env,
        r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
    );
    let source = get(&listing, "me.dev").source.clone();
    let refused = code(done);
    eprintln!(
        "REV2 git add with a tracked .flea-source: source {source:?}, update code {refused:?}"
    );
    assert_eq!(
        source,
        Source::Git(format!("file://{}", up.display())),
        "added from Git, so SOURCE names its origin"
    );
    assert_eq!(
        refused, "",
        "docs/protocol.md: update reaches a plugin added from Git"
    );
}

#[test]
fn rev2_st2_an_add_answers_when_the_cloned_source_file_links_to_a_fifo() {
    if super::fifotest::isolated("backend::rev2_st2_tests::rev2_st2_an_add_answers_when_the_cloned_source_file_links_to_a_fifo") {
        return;
    }
    let dir = TestDir::new("rev2-st2-fifo");
    let env: Env = env(&dir);
    let fifo = dir.join("pipe");
    mkfifo(&fifo);
    let up = upstream(&dir, |p| symlink(&fifo, p).unwrap());
    let line = add_line(&up);
    // Waits on the answer or on the request reading the fifo, never on a clock other tests share through CHANGING.
    let held = super::fifotest::answers_or_holds(&fifo, super::fifotest::Access::Read, move || {
        ask(&env, &line);
    })
    .is_none();
    eprintln!("REV2 add of a repo whose .flea-source links to a fifo: held the request (and CHANGING) on the fifo: {held}");
    assert!(
        !held,
        "the add request never answered: its closing listing reads the cloned .flea-source fifo"
    );
}

#[test]
fn rev2_st2_a_cloned_source_file_is_read_within_a_bound() {
    let dir = TestDir::new("rev2-st2-big");
    let env = env(&dir);
    // Sparse, so it costs no disk; /dev/zero in its place has no end at all.
    let big = dir.join("big");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(16 << 20)
        .unwrap();
    let up = upstream(&dir, |p| symlink(&big, p).unwrap());
    super::pluginadd::add(&env, &format!("file://{}", up.display())).expect("added");
    let listed = installed(&env)
        .into_iter()
        .find(|i| i.plugin.manifest.id == "me.dev")
        .expect("listed")
        .plugin
        .source;
    let read = match &listed {
        Source::Folder(text) => text.len(),
        _ => 0,
    };
    eprintln!("REV2 listing read {read} bytes of the cloned .flea-source link target");
    assert!(
        read <= 4096,
        "one listing read {read} bytes of a file the cloned repository named"
    );
}
