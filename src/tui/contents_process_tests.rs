use super::{
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    render,
    theme::Theme,
    wire::{text, Wire},
};
use crate::jsondoc::Json;
use std::{
    io,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

pub(super) struct Root(pub(super) PathBuf);
impl Root {
    pub(super) fn new() -> Self {
        let parent = std::env::current_dir()
            .unwrap()
            .join(".superpowers/m9-tui-08");
        std::fs::create_dir_all(&parent).unwrap();
        let made = Command::new("/usr/bin/mktemp")
            .arg("-d")
            .arg(parent.join("process.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let path = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
        assert!(path.is_absolute() && path.starts_with(parent));
        std::fs::write(
            path.join(".flea-test-sandbox"),
            "TUI contents process fixture\n",
        )
        .unwrap();
        for name in [
            "home", "config", "cache", "data", "state", "runtime", "tools",
        ] {
            std::fs::create_dir(path.join(name)).unwrap();
        }
        Self(path)
    }
    pub(super) fn command(&self, missing_rg: bool, test: &str) -> Command {
        let mut command = Command::new("/usr/bin/setsid");
        command
            .args([
                "-w",
                "/usr/bin/bwrap",
                "--unshare-user",
                "--unshare-pid",
                "--die-with-parent",
                "--ro-bind",
                "/",
                "/",
                "--proc",
                "/proc",
                "--dev-bind",
                "/dev",
                "/dev",
                "--bind",
            ])
            .arg(&self.0)
            .arg(&self.0)
            .arg("--chdir")
            .arg(&self.0)
            .arg("/usr/bin/timeout")
            .arg("60s")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", test, "--ignored", "--nocapture"])
            .env_clear()
            .env("FLEA_TUI_CONTENTS_CHILD", "1")
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .env(
                "PATH",
                if missing_rg {
                    self.0.join("tools")
                } else {
                    PathBuf::from("/usr/bin:/bin")
                },
            )
            .env("HOME", self.0.join("home"));
        for (key, dir) in [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_RUNTIME_DIR", "runtime"),
        ] {
            command.env(key, self.0.join(dir));
        }
        command
    }
    pub(super) fn backend(&self, missing_rg: bool) -> Wire {
        Wire::spawn(self.command(
            missing_rg,
            "tui::contents_process_tests::tui_contents_backend_child",
        ))
        .unwrap()
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty());
        assert!(self.0.starts_with(
            std::env::current_dir()
                .unwrap()
                .join(".superpowers/m9-tui-08")
        ));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
#[ignore = "private backend process entry point"]
fn tui_contents_backend_child() {
    if std::env::var_os("FLEA_TUI_CONTENTS_CHILD").is_some() {
        std::process::exit(crate::backend::run::run());
    }
}
pub(super) fn until(m: &mut Model, wire: &mut Wire, kind: &str) -> Json {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "backend deadline expired; last event: {last}"
        );
        let event = wire
            .events
            .recv_timeout(remaining)
            .expect("backend response before deadline");
        let value = match event {
            Ok(value) => value,
            Err(error) => {
                last = error;
                continue;
            }
        };
        last = crate::jsondoc::render(&value);
        let done = text(&value, "t") == kind;
        m.receive(value.clone(), wire).unwrap();
        if !m.error.is_empty() && kind != "error" {
            panic!("backend failed: {}", m.error);
        }
        if done {
            return value;
        }
    }
}
pub(super) fn find(m: &mut Model, wire: &mut Wire, value: &str) -> io::Result<()> {
    let map = Map::load();
    let keys = super::input::Decoder::default().feed(b"\x07", false);
    assert_eq!(keys.len(), 1, "terminal Ctrl+G emission");
    actions::key(m, &keys[0], &map, wire)?;
    m.editor
        .as_mut()
        .expect("real Ctrl+G Find")
        .replace(value.into());
    // Down commits the line and only shows the results; one Enter would also open the first of them.
    actions::key(m, &Key::named("Down", ""), &map, wire)
}
#[test]
fn tui_contents_real_process_finds_text_and_pages() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Root::new();
    for index in 0..7 {
        std::fs::write(
            root.0.join(format!("file-{index}.txt")),
            format!("unmatched\nneedle source {index}\nneedle later\n"),
        )
        .unwrap();
    }
    std::fs::write(root.0.join(".hidden.txt"), "needle hidden\n").unwrap();
    let mut wire = root.backend(false);
    let mut m = Model::new(root.0.clone(), &Json::Null);
    m.height = 4;
    find(&mut m, &mut wire, "needle").unwrap();
    until(&mut m, &mut wire, "searched");
    assert_eq!(m.total, 7, "all actual text files, without hidden files");
    assert_eq!(m.rows.len(), 2, "only a two-block viewport is retained");
    assert_eq!(m.find.matches[&0].line_no, 2);
    assert_eq!(m.find.matches[&0].line, "needle source 0");
    let frame = render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        120,
        12,
        Duration::ZERO,
    );
    assert!(render::frame_tests::plain(&frame).contains("needle source 0"));
    m.cursor = 6;
    m.window(&mut wire).unwrap();
    until(&mut m, &mut wire, "rows");
    assert_eq!(m.current_path(), Some(root.0.join("file-6.txt")));
    assert_eq!(m.find.matches[&6].line, "needle source 6");
    assert!(m.rows.len() <= 2);
    actions::key(&mut m, &Key::named("Escape", ""), &Map::load(), &mut wire).unwrap();
    until(&mut m, &mut wire, "rows");
    assert!(m.search.is_empty());
    assert!(m.find.active.is_none());
    eprintln!("TUI_CONTENTS_PROCESS PASS real Ctrl+G, first matching line, paging, cancel navigation, clean environment, private PID namespace");
}
#[test]
fn tui_contents_real_process_missing_rg_reports_failure() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Root::new();
    let mut wire = root.backend(true);
    let mut m = Model::new(root.0.clone(), &Json::Null);
    find(&mut m, &mut wire, "needle").unwrap();
    let error = until(&mut m, &mut wire, "error");
    assert_eq!(text(&error, "where"), "searchcontents");
    assert!(m.error.contains("ripgrep"), "{}", m.error);
    assert!(!m.searching && !m.find.waiting);
    assert!(
        m.search.is_empty(),
        "failure is not a successful empty search"
    );
    eprintln!("TUI_CONTENTS_PROCESS PASS real missing ripgrep refusal, no fallback");
}

#[test]
fn tui_contents_real_process_cancels_the_shared_worker() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let root = Root::new();
    let tool = root.0.join("tools/rg");
    std::fs::write(&tool, "#!/bin/sh\nexec /usr/bin/sleep 30\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    for program in ["bwrap", "prlimit"] {
        std::os::unix::fs::symlink(
            format!("/usr/bin/{program}"),
            root.0.join("tools").join(program),
        )
        .unwrap();
    }
    let mut wire = root.backend(true);
    let mut m = Model::new(root.0.clone(), &Json::Null);
    find(&mut m, &mut wire, "needle").unwrap();
    actions::key(&mut m, &Key::named("Escape", ""), &Map::load(), &mut wire).unwrap();
    let answer = until(&mut m, &mut wire, "searched");
    assert!(answer
        .get("cancelled")
        .and_then(Json::as_bool)
        .unwrap_or(false));
    assert!(!m.searching && !m.find.waiting);
    assert!(m.search.is_empty());
    eprintln!("TUI_CONTENTS_PROCESS PASS real shared-worker cancellation, cleanup acknowledgement");
}
