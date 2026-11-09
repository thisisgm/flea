// Review-only probes for g2-st2 round 3: the Git add's caps, the source record's writes, and a crafted .git config.
use super::fifotest::mkfifo;
use super::plugins_tests::{ask, code, env, get, plugin, READY};
use super::testdir::TestDir;
use flea::ui_logic_plugin_settings::wire::Source;
use std::os::unix::fs::symlink;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

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

fn repo(dir: &TestDir, id: &str, extra: impl FnOnce(&TestDir)) -> PathBuf {
    let up = plugin(dir, "up", id, READY);
    extra(dir);
    git(&up, &["init", "--quiet", "--initial-branch=main"]);
    git(&up, &["add", "-A"]);
    git(&up, &["commit", "--quiet", "-m", "one"]);
    up
}

fn add_line(source: &str) -> String {
    format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{source}"}}"#)
}

fn entries(path: &Path) -> usize {
    std::fs::read_dir(path).map(|d| d.count()).unwrap_or(0)
}

#[test]
fn rev3_st2_a_git_add_past_the_entry_cap_in_a_dot_folder_never_lands() {
    let dir = TestDir::new("rev3-st2-dotmany");
    let env = env(&dir);
    let up = repo(&dir, "me.many", |d| {
        d.dir("up/.many");
        for n in 0..4200 {
            d.file(&format!("up/.many/{n}"), "");
        }
    });
    let refused = code(ask(&env, &add_line(&format!("file://{}", up.display()))).0);
    let landed = entries(&env.dir.join("me.many/.many"));
    eprintln!("REV3 git add with 4200 files under .many: code {refused:?}, {landed} entries landed in the store");
    assert_eq!(
        refused, "write",
        "docs/protocol.md: a Git add checks its clone against 4096 entries, its own .git aside"
    );
    assert!(!env.dir.join("me.many").exists(), "nothing lands");
}

#[test]
fn rev3_st2_a_git_add_past_the_byte_cap_in_a_dot_file_never_lands() {
    let dir = TestDir::new("rev3-st2-dotbig");
    let env = env(&dir);
    let up = repo(&dir, "me.big", |d| {
        std::fs::File::create(d.join("up/.blob"))
            .unwrap()
            .set_len(65 << 20)
            .unwrap();
    });
    let refused = code(ask(&env, &add_line(&format!("file://{}", up.display()))).0);
    let landed = std::fs::metadata(env.dir.join("me.big/.blob"))
        .map(|m| m.len())
        .unwrap_or(0);
    eprintln!(
        "REV3 git add with a 65 MiB .blob: code {refused:?}, {landed} bytes landed in the store"
    );
    assert_eq!(
        refused, "write",
        "docs/protocol.md: a Git add checks its clone against 64 MB, its own .git aside"
    );
    assert!(!env.dir.join("me.big").exists(), "nothing lands");
}

#[test]
fn rev3_st2_control_a_git_add_past_the_byte_cap_in_a_plain_file_is_refused() {
    let dir = TestDir::new("rev3-st2-plainbig");
    let env = env(&dir);
    let up = repo(&dir, "me.big", |d| {
        std::fs::File::create(d.join("up/blob"))
            .unwrap()
            .set_len(65 << 20)
            .unwrap();
    });
    let refused = code(ask(&env, &add_line(&format!("file://{}", up.display()))).0);
    eprintln!("REV3 control, git add with a 65 MiB blob: code {refused:?}");
    assert_eq!(refused, "write");
    assert!(!env.dir.join("me.big").exists());
}

#[test]
fn rev3_st2_a_folder_add_never_writes_through_an_old_record() {
    if super::fifotest::isolated(
        "backend::rev3_st2_tests::rev3_st2_a_folder_add_never_writes_through_an_old_record",
    ) {
        return;
    }
    let dir = TestDir::new("rev3-st2-rec");
    let env = env(&dir);
    let outside = dir.file("outside", "keep\n");
    let hard = dir.file("hardsrc", "keep too\n");
    let sources = dir.dir("config/flea/plugins/.sources");
    symlink(&outside, sources.join("me.tool")).unwrap();
    std::fs::hard_link(&hard, sources.join("me.other")).unwrap();
    let fifo = sources.join("me.third");
    mkfifo(&fifo);
    let mut added = Vec::new();
    for (folder, id) in [("a", "me.tool"), ("b", "me.other"), ("c", "me.third")] {
        let src = plugin(&dir, folder, id, READY);
        let (env2, line) = (env_of(&dir), add_line(&src.display().to_string()));
        // Waits on the answer or on the add opening the old fifo record, never on a clock CHANGING's queue eats.
        let done =
            super::fifotest::answers_or_holds(&fifo, super::fifotest::Access::Write, move || {
                code(ask(&env2, &line).0)
            })
            .unwrap_or_else(|| {
                panic!("a folder add over an old record waited on the old fifo record")
            });
        assert_eq!(done, "", "{id} added");
        added.push((id, src));
    }
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "keep\n",
        "a linked record's target is untouched"
    );
    assert_eq!(
        std::fs::read_to_string(&hard).unwrap(),
        "keep too\n",
        "a hard-linked record's other name is untouched"
    );
    let listing = ask(&env, r#"{"c":"plugins","op":"list","id":9}"#).1;
    for (id, src) in added {
        let meta = std::fs::symlink_metadata(sources.join(id)).unwrap();
        assert!(
            meta.file_type().is_file() && meta.nlink() == 1 && meta.mode() & 0o777 == 0o600,
            "{id} record is a fresh 0600 file"
        );
        assert_eq!(
            get(&listing, id).source,
            Source::Folder(src.display().to_string())
        );
    }
}

#[test]
fn rev3_st2_a_linked_sources_folder_is_refused_and_written_nowhere() {
    let dir = TestDir::new("rev3-st2-srclink");
    let env = env(&dir);
    let elsewhere = dir.dir("elsewhere");
    dir.dir("config/flea/plugins");
    symlink(&elsewhere, env.dir.join(".sources")).unwrap();
    let src = plugin(&dir, "src", "me.tool", READY);
    let refused = code(ask(&env, &add_line(&src.display().to_string())).0);
    eprintln!(
        "REV3 folder add over a linked .sources: code {refused:?}, {} entries elsewhere",
        entries(&elsewhere)
    );
    assert_eq!(refused, "write");
    assert_eq!(entries(&elsewhere), 0);
    assert!(!env.dir.join("me.tool").exists());
}

#[test]
fn rev3_st2_update_with_a_crafted_ext_remote_runs_nothing() {
    let dir = TestDir::new("rev3-st2-ext");
    let env = env(&dir);
    let up = repo(&dir, "me.dev", |_| {});
    assert_eq!(
        code(ask(&env, &add_line(&format!("file://{}", up.display()))).0),
        ""
    );
    let marker = dir.join("ext-ran");
    let ext = format!("ext::sh -c touch% {}", marker.display());
    let installed = env.dir.join("me.dev");
    let cases: [&[(&str, &str)]; 2] = [
        &[("branch.main.remote", ext.as_str())],
        &[
            ("branch.main.remote", "origin"),
            ("remote.origin.url", ext.as_str()),
        ],
    ];
    for case in cases {
        for (key, value) in case {
            git(&installed, &["config", key, value]);
        }
        let key = case[case.len() - 1].0;
        dir.file("up/later.txt", key);
        git(&up, &["add", "-A"]);
        git(&up, &["commit", "--quiet", "-m", key]);
        let refused = code(
            ask(
                &env,
                r#"{"c":"plugins","op":"update","id":2,"plugin":"me.dev"}"#,
            )
            .0,
        );
        eprintln!(
            "REV3 update with {key} = ext: code {refused:?}, marker {}",
            marker.exists()
        );
        assert!(!marker.exists(), "{key}: an ext:: transport ran");
    }
}

#[test]
fn rev3_st2_a_listing_answers_when_a_git_plugin_config_is_a_fifo() {
    let dir = TestDir::new("rev3-st2-cfgfifo");
    let env = env(&dir);
    let up = repo(&dir, "me.dev", |_| {});
    assert_eq!(
        code(ask(&env, &add_line(&format!("file://{}", up.display()))).0),
        ""
    );
    let config = env.dir.join("me.dev/.git/config");
    dir.assert_contains(&config);
    std::fs::remove_file(&config).unwrap();
    mkfifo(&config);
    let (tx, rx) = std::sync::mpsc::channel();
    let env2 = env_of(&dir);
    std::thread::spawn(move || {
        let _ = tx.send(
            ask(&env2, r#"{"c":"plugins","op":"list","id":3}"#)
                .1
                .plugins
                .len(),
        );
    });
    let held = rx.recv_timeout(Duration::from_secs(5)).is_err();
    if held {
        let writer = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(crate::oflags::O_NONBLOCK)
            .open(&config);
        drop(writer);
        let _ = rx.recv_timeout(Duration::from_secs(5));
    }
    eprintln!("REV3 listing with a fifo .git/config: held past 5 s: {held}");
    assert!(
        !held,
        "the listing never answered: source_of reads .git/config blocking"
    );
}

fn env_of(dir: &TestDir) -> super::plugins::Env {
    env(dir)
}
