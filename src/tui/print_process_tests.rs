use super::{
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    wire::{self, Wire},
};
use crate::{backend::testdir::TestDir, jsondoc::Json};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

const STUB: &str = r#"#!/bin/sh
set -eu
root=${FLEA_TUI_PRINT_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
case "${0##*/}" in
 prlimit) while [ "${1#--}" != "$1" ]; do shift; done; exec "$@" ;;
 bwrap)
  while [ "$#" -gt 0 ]; do
   case "$1" in
    --ro-bind|--ro-bind-try|--bind|--symlink|--setenv) shift 3 ;;
    --tmpfs|--proc|--dev|--json-status-fd) shift 2 ;;
    --*) shift ;;
    *) exec "$@" ;;
   esac
  done ;;
 lpstat) case "$1" in -e) printf 'Brother\n' ;; -d) printf 'system default destination: Brother\n' ;; esac ;;
 lpoptions)
  if test -f "$root/no-duplex"; then printf 'Duplex/Two-sided: *None\nColorModel/Color: *Gray\n'
  else printf 'Duplex/Two-sided: *None DuplexNoTumble\nColorModel/Color: *RGB Gray\n'; fi ;;
 lp)
  printf '%s\0' "$@" >> "$root/jobs"
  printf '\n' >> "$root/jobs"
  case "$*" in *hold.pdf*) printf '%s\n' "$$" > "$root/held-pid"; touch "$root/held"; exec /bin/sleep 30 ;; *refuse.pdf*) printf 'queue refused\n' >&2; exit 1 ;; esac ;;
 system-config-printer) printf 'setup\n' > "$root/setup-recorded" ;;
 *) exit 99 ;;
esac
"#;

fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
fn until(m: &mut Model, w: &mut Wire, done: impl Fn(&Model) -> bool) {
    let end = std::time::Instant::now() + Duration::from_secs(15);
    while !done(m) {
        let event = w
            .events
            .recv_timeout(end.saturating_duration_since(std::time::Instant::now()))
            .expect("backend response deadline")
            .unwrap();
        m.receive(event, w).unwrap();
    }
}
fn model(root: &Path, w: &mut Wire, names: &[&str]) -> Model {
    let mut m = Model::new(root.into(), &Json::Null);
    m.open(root.into(), w).unwrap();
    until(&mut m, w, |m| {
        m.rows.len() == m.total && m.total > 0 && m.pending.is_none()
    });
    m.selected = m
        .rows
        .iter()
        .filter(|(_, row)| names.contains(&row.name.as_str()))
        .map(|(i, _)| *i)
        .collect();
    assert_eq!(m.selected.len(), names.len());
    m.cursor = *m.selected.first().unwrap();
    m.remember_selection();
    press(&mut m, w, Key::character('P', "ctrl"));
    until(&mut m, w, |m| {
        m.printing.card.as_ref().is_some_and(|f| f.queried)
    });
    m
}

#[test]
#[ignore = "private backend process entry point"]
fn tui_print_backend_helper() {
    if std::env::var_os("FLEA_TUI_PRINT_BACKEND").is_some() {
        std::process::exit(crate::backend::run::run());
    }
}

#[test]
#[ignore = "private clean-environment process entry point"]
fn tui_print_process_helper() {
    let Some(root) = std::env::var_os("FLEA_TUI_PRINT_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    assert!(root.join(".flea-test-sandbox").is_file());
    for state in ["/run/cups", "/var/lib/cups", "/var/cache/cups", "/var/spool/cups", "/etc/cups", "/etc/printcap"] {
        assert!(!Path::new(state).exists(), "host CUPS state must be hidden: {state}");
    }
    let executable = std::env::current_exe().unwrap();
    let inputs = [root.as_path(), Path::new(env!("CARGO_MANIFEST_DIR")), executable.parent().unwrap()];
    let mut hidden = ["/run", "/var", "/etc"].map(std::path::PathBuf::from).to_vec();
    assert!(inputs.iter().all(|input| hidden.iter().all(|path| !path.starts_with(input))));
    while let Some(directory) = hidden.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if inputs.contains(&path.as_path()) {
                continue;
            }
            assert!(inputs.iter().any(|input| input.starts_with(&path)), "host CUPS state must be hidden: {}", path.display());
            assert!(path.is_dir() && !path.is_symlink(), "only namespace input parents may be visible");
            hidden.push(path);
        }
    }
    let namespace = std::fs::read_link("/proc/self/ns/net").unwrap();
    assert_ne!(
        namespace.to_str().unwrap(),
        std::env::var("FLEA_HOST_NET_NS").unwrap()
    );
    let command = || {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", r#""$1" --exact tui::print_process_tests::tui_print_backend_helper --ignored --nocapture --format=terse --test-threads=1 | /usr/bin/sed -u -n '/^{/p'"#, "tui-print-backend"])
            .arg(std::env::current_exe().unwrap()).env("FLEA_TUI_PRINT_BACKEND", "1");
        c
    };
    let files = root.join("files");
    let mut w = Wire::spawn(command()).unwrap();
    let mut m = model(
        &files,
        &mut w,
        &[
            "report.pdf",
            "refuse.pdf",
            "photo.png",
            "code.rs",
            "notes.txt",
        ],
    );
    let f = m.printing.card.as_mut().unwrap();
    f.copies = "2".into();
    f.pages = "1-3, 5".into();
    f.two_sided = true;
    f.grayscale = true;
    press(&mut m, &mut w, Key::named("Return", ""));
    until(&mut m, &mut w, |m| m.printing.run.is_none());
    assert!(m.error.contains("queue refused"));
    let jobs = std::fs::read_to_string(root.join("jobs")).unwrap();
    assert_eq!(jobs.lines().count(), 5, "one CUPS job per selected file");
    assert!(jobs
        .lines()
        .find(|s| s.contains("photo.png"))
        .unwrap()
        .contains("fit-to-page\0--\0"));
    assert!(jobs
        .lines()
        .find(|s| s.contains("report.pdf"))
        .unwrap()
        .contains("-P\x001-3,5\0"));
    for job in jobs.lines() {
        let argv: Vec<_> = job.split('\0').collect();
        for pair in [
            ["-n", "2"],
            ["-o", "sides=two-sided-long-edge"],
            ["-o", "print-color-mode=monochrome"],
        ] {
            assert_eq!(
                argv.windows(2).filter(|args| *args == pair).count(),
                1,
                "missing or duplicate option {pair:?}: {argv:?}"
            );
        }
    }
    assert_eq!(
        std::fs::read(files.join("report.pdf")).unwrap(),
        b"held source"
    );
    assert_eq!(
        std::fs::read(files.join("refuse.pdf")).unwrap(),
        b"held source"
    );
    let mut m = model(&files, &mut w, &["report.pdf"]);
    std::fs::write(root.join("replacement"), b"new identity").unwrap();
    std::fs::rename(root.join("replacement"), files.join("report.pdf")).unwrap();
    press(&mut m, &mut w, Key::named("Return", ""));
    until(&mut m, &mut w, |m| m.printing.run.is_none());
    assert!(!m.error.is_empty());
    assert_eq!(
        std::fs::read_to_string(root.join("jobs")).unwrap(),
        jobs,
        "stale selection cannot submit"
    );
    let mut m = model(&files, &mut w, &["notes.txt"]);
    m.printing.card.as_mut().unwrap().two_sided = true;
    std::fs::write(root.join("no-duplex"), b"1").unwrap();
    press(&mut m, &mut w, Key::named("Return", ""));
    until(&mut m, &mut w, |m| m.printing.run.is_none());
    assert!(
        m.error.contains("two-sided"),
        "capability changes revalidated: {}",
        m.error
    );
    assert_eq!(std::fs::read_to_string(root.join("jobs")).unwrap(), jobs);
    let mut m = model(&files, &mut w, &["hold.pdf"]);
    super::print_fix_tests::cancellation_catalog(&mut m, &root);
    press(&mut m, &mut w, Key::named("Return", ""));
    until(&mut m, &mut w, |m| m.printing.card.is_none());
    wait_for(|| root.join("held").is_file());
    let pid = std::fs::read_to_string(root.join("held-pid")).unwrap();
    let process = std::path::PathBuf::from(format!("/proc/{}", pid.trim()));
    assert!(process.is_dir(), "stub is running before Escape");
    press(&mut m, &mut w, Key::named("Escape", ""));
    until(&mut m, &mut w, |m| m.printing.run.is_none());
    wait_for(|| !process.exists());
    assert_eq!(
        m.message,
        "Printing cancelled. Sent: None. Not sent: hold.pdf."
    );
    drop(w);
    let mut m = super::render::frame_tests::model();
    let (mut w, reader) = wire::echo_wire();
    press(&mut m, &mut w, Key::character('P', "ctrl"));
    super::action_key_tests::drain(&mut w);
    let id = m.action_id;
    for line in [
        format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true}}"#),
        format!(
            r#"{{"t":"menuaction","op":"validate","action":"print","id":{id},"ok":true,"paths":["/fx/v/work/notes.txt"]}}"#
        ),
    ] {
        m.receive(crate::jsondoc::parse(&line).unwrap(), &mut w)
            .unwrap();
    }
    let query = m.printing.card.as_ref().unwrap().query.unwrap();
    m.receive(crate::jsondoc::parse(&format!(r#"{{"t":"printoptions","id":{query},"ok":true,"destinations":[],"printer":"","duplex":false,"twoSided":false,"color":false,"grayscale":false,"setupArgv":["system-config-printer"]}}"#)).unwrap(), &mut w).unwrap();
    press(&mut m, &mut w, Key::named("Return", ""));
    for (_, child) in &mut m.launches {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(
        std::fs::read_to_string(root.join("setup-recorded")).unwrap(),
        "setup\n"
    );
    wire::finish(w, reader);
}

#[test]
fn tui_print_real_backend_and_private_setup_recorder() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let dir = TestDir::new("tui-print");
    dir.dir("files");
    for name in [
        "report.pdf",
        "refuse.pdf",
        "hold.pdf",
        "photo.png",
        "notes.txt",
        "code.rs",
    ] {
        dir.file(&format!("files/{name}"), "held source");
    }
    let bin = dir.dir("bin");
    let stub = dir.script("tools", STUB);
    for name in [
        "lpstat",
        "lpoptions",
        "lp",
        "prlimit",
        "bwrap",
        "system-config-printer",
    ] {
        std::os::unix::fs::symlink(&stub, bin.join(name)).unwrap();
    }
    for name in ["touch", "cat", "chmod", "sh"] {
        std::os::unix::fs::symlink(format!("/usr/bin/{name}"), bin.join(name)).unwrap();
    }
    // The private /var must still expose this test's executable and English catalog.
    let executable = std::env::current_exe().unwrap();
    let mut command = Command::new("/usr/bin/setsid");
    command
        .args([
            "-w",
            "/usr/bin/bwrap",
            "--unshare-user",
            "--unshare-pid",
            "--unshare-net",
            "--die-with-parent",
            "--ro-bind",
            "/",
            "/",
            "--tmpfs",
            "/run",
            "--tmpfs",
            "/var",
            "--tmpfs",
            "/etc",
            "--proc",
            "/proc",
            "--dev-bind",
            "/dev",
            "/dev",
            "--ro-bind",
        ])
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("--ro-bind")
        .arg(executable.parent().unwrap())
        .arg(executable.parent().unwrap())
        .arg("--bind")
        .arg(dir.path())
        .arg(dir.path())
        .arg(&executable);
    command
        .args([
            "--exact",
            "tui::print_process_tests::tui_print_process_helper",
            "--ignored",
            "--nocapture",
        ])
        .env_clear()
        .env("FLEA_TUI_PRINT_ROOT", dir.path())
        .env(
            "FLEA_HOST_NET_NS",
            std::fs::read_link("/proc/self/ns/net").unwrap(),
        )
        .env("PATH", bin.as_os_str())
        .env("HOME", dir.dir("home"))
        .env("TMPDIR", dir.dir("tmp"))
        .env(
            "FLEA_I18N",
            crate::i18n::test_catalogs::english_only(),
        )
        .env("LC_ALL", "C.UTF-8")
        .env("CUPS_SERVER", dir.join("private-cups.sock"))
        .stdin(Stdio::null());
    for (key, name) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        command.env(key, dir.dir(name));
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "helper must run exactly once: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(dir.join("held").is_file() && dir.join("jobs").is_file());
    assert_eq!(
        std::fs::read_to_string(dir.join("setup-recorded")).unwrap(),
        "setup\n"
    );
}

fn wait_for(done: impl Fn() -> bool) {
    let end = std::time::Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(
            std::time::Instant::now() < end,
            "print process state deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
