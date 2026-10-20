// Add and Update from Git, and the plugins request on a real backend's own stdin and stdout.
use super::plugins_tests::{ask, code, env, get, plugin, READY};
use super::testdir::TestDir;
use flea::ui_logic_st2::wire::{self, Reply, Source};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

const CHILD: &str = "FLEA_PLUGINS_PROTOCOL_CHILD";
const CHILD_TEST: &str = "backend::pluginadd_tests::backend_child";

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false"])
        .args(args).env("GIT_CONFIG_GLOBAL", "/dev/null").stdout(Stdio::null()).stderr(Stdio::null()).status().expect("git runs");
    assert!(status.success(), "git {args:?}");
}

#[test]
fn st2_backend_a_git_add_names_its_origin_and_update_pulls_the_new_version() {
    let dir = TestDir::new("st2-git");
    let env = env(&dir);
    let repo = plugin(&dir, "repo", "me.gitted", READY);
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "one"]);
    let url = format!("file://{}", repo.display());
    let (done, listing) = ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{url}"}}"#));
    assert_eq!(code(done), "");
    assert_eq!(get(&listing, "me.gitted").source, Source::Git(url.clone()));
    let manifest = repo.join("manifest.json");
    let bumped = std::fs::read_to_string(&manifest).unwrap().replace("\"1.0.0\"", "\"1.1.0\"");
    dir.replace_file(&manifest, &bumped);
    git(&repo, &["commit", "--quiet", "-am", "two"]);
    let (done, listing) = ask(&env, r#"{"c":"plugins","op":"update","id":2,"plugin":"me.gitted"}"#);
    let done = done.unwrap();
    assert_eq!((done.error, done.version.as_str()), (None, "1.1.0"));
    assert_eq!(get(&listing, "me.gitted").manifest.version, "1.1.0");
    std::fs::remove_dir_all({ dir.assert_contains(&repo); &repo }).unwrap();
    let (failed, _) = ask(&env, r#"{"c":"plugins","op":"update","id":3,"plugin":"me.gitted"}"#);
    assert_eq!(failed.unwrap().error.map(|e| (e.code, e.detail)), Some(("git", url)), "an origin that is gone cannot be fetched");
    let (refused, _) = ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":4,"source":"file://{}"}}"#, dir.join("nowhere").display()));
    assert_eq!(code(refused), "git");
}

#[test]
fn st2_backend_update_refuses_a_plugin_added_from_a_folder() {
    let dir = TestDir::new("st2-noupd");
    let env = env(&dir);
    let source = plugin(&dir, "src", "me.local", READY);
    ask(&env, &format!(r#"{{"c":"plugins","op":"add","id":1,"source":"{}"}}"#, source.display()));
    assert_eq!(code(ask(&env, r#"{"c":"plugins","op":"update","id":2,"plugin":"me.local"}"#).0), "git");
}

// Re-execute the real CLI entry point with --backend in an isolated process.
#[test]
fn backend_child() {
    if std::env::var_os(CHILD).is_some() {
        crate::main();
    }
}

#[test]
fn st2_backend_protocol_a_real_backend_answers_plugins_lines_off_its_loop() {
    let dir = TestDir::new("st2-proto");
    let source = plugin(&dir, "src", "me.wired", READY);
    let mut child = Command::new("timeout").arg("60").arg(std::env::current_exe().unwrap())
        .args([CHILD_TEST, "--exact", "--nocapture", "--", "--backend"])
        .env(CHILD, "1").env("HOME", dir.path())
        .env("XDG_CACHE_HOME", dir.join("cache")).env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_STATE_HOME", dir.join("state")).env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_RUNTIME_DIR", dir.path()).env("OMARCHY_PATH", dir.join("omarchy"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().expect("backend child");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut next = |kind: &str| loop {
        let line = out.next().expect("the backend answers").unwrap();
        if let Some(at) = line.find("{\"t\":") {
            let line = line[at..].to_string();
            if wire::parse(&line).is_ok() && line.contains(&format!("\"t\":\"{kind}\"")) {
                return line;
            }
        }
    };
    writeln!(stdin, r#"{{"c":"plugins","op":"add","id":11,"source":"{}"}}"#, source.display()).unwrap();
    let outcome = next(wire::RESULT);
    assert!(outcome.contains("\"id\":11") && outcome.contains("\"ok\":true"), "{outcome}");
    let Ok(Reply::Listing(listing)) = wire::parse(&next(wire::LISTING)) else { panic!() };
    assert_eq!(listing.id, 11);
    assert!(!get(&listing, "me.wired").enabled);
    writeln!(stdin, r#"{{"c":"plugins","op":"enable","id":12,"plugin":"me.wired"}}"#).unwrap();
    assert!(next(wire::RESULT).contains("\"ok\":true"));
    let Ok(Reply::Listing(listing)) = wire::parse(&next(wire::LISTING)) else { panic!() };
    assert!(get(&listing, "me.wired").enabled && get(&listing, "me.wired").failure.is_none());
    writeln!(stdin, "{{\"c\":\"quit\"}}").unwrap();
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let state = std::fs::read_to_string(dir.join("state/flea/ui.json")).unwrap();
    assert!(state.contains("\"me.wired\""), "the switch reached the real ui.json: {state}");
}
