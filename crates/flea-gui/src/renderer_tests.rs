use super::{finish_opening, renderer};
use gpui::WindowingRequest;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const CHILD: &str = "FLEA_GUI_NEITHER_STARTUP_CHILD";
const TEST: &str = "renderer_tests::neither_renderer_exits_during_startup";

#[test]
fn neither_renderer_exits_during_startup() {
    if std::env::var_os(CHILD).as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(5));
            eprintln!("renderer regression: startup failure waited for the event loop");
            std::process::exit(77);
        });
        gpui_platform::application()
            .with_windowing(WindowingRequest::Headless)
            .run(|_| {
                finish_opening(Err::<(), ()>(()));
            });
        eprintln!("renderer regression: failed startup returned from Application::run");
        std::process::exit(79);
    }

    let created = Command::new("mktemp")
        .arg("-d")
        .arg(std::env::temp_dir().join("flea-render-startup.XXXXXXXX"))
        .output()
        .expect("create an isolated renderer fixture");
    assert!(created.status.success());
    let root = PathBuf::from(std::str::from_utf8(&created.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
    std::fs::write(root.join(".flea-test-sandbox"), b"renderer startup test\n").unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, "1")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY");
    for (key, name) in [
        ("HOME", "home"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        let directory = root.join(name);
        std::fs::create_dir(&directory).unwrap();
        command.env(key, directory);
    }
    let output = command.output().expect("run the headless startup child");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(renderer::UNAVAILABLE),
        "headless startup child failed: {stderr}"
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|line| *line == renderer::MESSAGE)
            .count(),
        1,
        "{stderr}"
    );
    assert!(!stderr.contains("renderer regression:"), "{stderr}");
}
