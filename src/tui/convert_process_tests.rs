//! Real converters run in a private process environment and never see a printer.
use super::{actions, input::Key, keymap::Map, model::Model, wire::Wire};
use crate::{backend::testdir::TestDir, jsondoc::Json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn tool(args: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/timeout")
        .args(["20", args[0]])
        .args(&args[1..])
        .output()
        .expect("real converter must be installed");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
pub fn scenario(case: &str) {
    let dir = TestDir::new("tui-convert-real");
    let bin = dir.dir("bin");
    for name in [
        "home", "config", "cache", "data", "state", "runtime", "files",
    ] {
        dir.dir(name);
    }
    std::os::unix::fs::symlink("/usr/bin/true", bin.join("soffice")).unwrap();
    dir.dir("i18n/en");
    dir.file("i18n/en/flea.json", r#"{"tui.convert.cancelled":{"one":"Conversion cancelled; %1 output created.","other":"Conversion cancelled; %1 outputs created."},"tui.convert.failed":{"one":"Conversion failed after %1 output: %2","other":"Conversion failed after %1 outputs: %2"}}"#);
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
            "--proc",
            "/proc",
            "--dev-bind",
            "/dev",
            "/dev",
            "--bind",
        ])
        .arg(dir.path())
        .arg(dir.path())
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tui::convert_process_tests::tui_convert_process_helper",
            "--ignored",
            "--nocapture",
        ])
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/local/bin:/usr/bin:/bin", bin.display()),
        )
        .env("FLEA_TUI_CONVERT_ROOT", dir.path())
        .env("FLEA_TUI_CONVERT_CASE", case)
        .env("FLEA_I18N", dir.join("i18n"))
        .env("MAGICK_THREAD_LIMIT", "1")
        .env("OMP_NUM_THREADS", "1")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null());
    for (key, name) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        command.env(key, dir.join(name));
    }
    assert!(
        command.status().unwrap().success(),
        "real conversion scenario {case}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("executed")).unwrap(),
        case,
        "the private helper must actually run"
    );
}
pub fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
pub fn until(m: &mut Model, w: &mut Wire, done: impl Fn(&Model) -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !done(m) {
        super::convert::poll(m);
        match w.events.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => m.receive(event.unwrap(), w).unwrap(),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(Instant::now() < end, "conversion deadline: {}", m.error)
            }
            Err(error) => panic!("backend: {error}"),
        }
    }
}
pub fn open(root: &Path, w: &mut Wire, names: &[&str]) -> Model {
    *w = backend();
    let mut m = Model::new(root.into(), &Json::Null);
    m.height = 27;
    m.columns = 130;
    m.reverse = std::env::var("FLEA_TUI_CONVERT_CASE").as_deref() == Ok("order");
    m.open(root.into(), w).unwrap();
    until(&mut m, w, |m| {
        m.pending.is_none() && m.total > 0 && m.rows.len() == m.total
    });
    m.selected = m
        .rows
        .iter()
        .filter(|(_, r)| names.contains(&r.name.as_str()))
        .map(|(i, _)| *i)
        .collect();
    assert_eq!(m.selected.len(), names.len());
    m.cursor = *m.selected.first().unwrap();
    m.remember_selection();
    press(&mut m, w, Key::character('F', ""));
    until(&mut m, w, |m| {
        m.menu_action == "convertCard" || !m.error.is_empty()
    });
    assert!(m.error.is_empty(), "{}", m.error);
    m
}
pub fn submit(m: &mut Model, w: &mut Wire) {
    press(m, w, Key::named("Return", ""));
    assert!(m.converting.run.is_some(), "conversion was not submitted");
    until(m, w, |m| m.converting.run.is_none());
}
pub fn fixture(root: &Path, name: &str, color: &str) -> PathBuf {
    let source = root.join(name);
    tool(&[
        "magick",
        "-size",
        "12x8",
        &format!("xc:{color}"),
        "-set",
        "comment",
        "retained-metadata",
        source.to_str().unwrap(),
    ]);
    source
}
pub fn pdf(root: &Path, name: &str) -> PathBuf {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>".into(),
    ];
    for (i, rgb) in ["1 0 0", "0 1 0", "0 0 1"].iter().enumerate() {
        let stream = format!("{rgb} rg 0 0 72 72 re f");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents {} 0 R >>",
            4 + i * 2
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    let mut body = "%PDF-1.4\n".to_string();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(body.len());
        body.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = body.len();
    body.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()));
    for offset in offsets.iter().skip(1) {
        body.push_str(&format!("{offset:010} 00000 n \n"));
    }
    body.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len()
    ));
    let path = root.join(name);
    std::fs::write(&path, body).unwrap();
    path
}
#[test]
#[ignore = "private backend entry point"]
fn tui_convert_backend_helper() {
    if std::env::var_os("FLEA_TUI_CONVERT_BACKEND").is_some() {
        std::process::exit(crate::backend::run::run());
    }
}
#[test]
#[ignore = "private clean conversion scenarios"]
fn tui_convert_process_helper() {
    let root = PathBuf::from(std::env::var_os("FLEA_TUI_CONVERT_ROOT").unwrap());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let mut w = backend();
    match std::env::var("FLEA_TUI_CONVERT_CASE").unwrap().as_str() {
        "formats" => super::convert_real_tests::formats(&root, &mut w),
        "pdf" => super::convert_real_tests::pages(&root, &mut w),
        "order" => super::convert_real_tests::order(&root, &mut w),
        "office" => super::convert_real_tests::office(&root, &mut w),
        "safe" => super::convert_real_tests::safe(&root, &mut w),
        _ => panic!("unknown scenario"),
    }
    drop(w);
    std::fs::write(
        root.join("executed"),
        std::env::var("FLEA_TUI_CONVERT_CASE").unwrap(),
    )
    .unwrap();
}

fn backend() -> Wire {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", r#""$1" --exact tui::convert_process_tests::tui_convert_backend_helper --ignored --nocapture --quiet | /usr/bin/sed -u -n '/^{/p'"#, "tui-convert-backend"])
        .arg(std::env::current_exe().unwrap()).env("FLEA_TUI_CONVERT_BACKEND", "1");
    Wire::spawn(command).unwrap()
}
