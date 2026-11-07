use super::{finish_opening, renderer};
use flea::i18n::{I18n, Paths};
use flea_gui::driver_notice::DriverNotice;
use gpui::WindowingRequest;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const CHILD: &str = "FLEA_GUI_NEITHER_STARTUP_CHILD";
const CAUSE: &str = "g76-window-open sentinel: Failed to open window (device=diagnostic-test)";
const TEST: &str = "renderer_tests::neither_renderer_exits_during_startup";

// No catalog loads from these paths, so the notice is the English fallback.
fn english_notice() -> DriverNotice {
    DriverNotice::new(&I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/flea-test/omarchy"),
        language_file: PathBuf::from("/nonexistent/flea-test/omarchy/language"),
        catalogs: Vec::new(),
    }))
}

#[test]
fn neither_renderer_exits_during_startup() {
    if std::env::var_os(CHILD).as_deref() == Some(std::ffi::OsStr::new("success")) {
        let window = String::from("window success sentinel");
        let pointer = window.as_ptr();
        let returned = finish_opening(Ok::<_, String>(window), &english_notice());
        assert_eq!(returned, "window success sentinel");
        assert_eq!(returned.as_ptr(), pointer);
        println!("renderer success: unchanged");
        return;
    }
    if std::env::var_os(CHILD).as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(5));
            eprintln!("renderer regression: startup failure waited for the event loop");
            std::process::exit(77);
        });
        gpui_platform::application()
            .with_windowing(WindowingRequest::Headless)
            .run(|_| {
                finish_opening(Err::<(), String>(CAUSE.to_owned()), &english_notice());
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
    // A stand-in notify-send records the desktop notice, so a test run never shows a real one.
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let notices = root.join("notice.log");
    let stand_in = bin.join("notify-send");
    std::fs::write(
        &stand_in,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\n",
            notices.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o700)).unwrap();
    command.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
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
    let success = command
        .env(CHILD, "success")
        .output()
        .expect("run the successful startup child");
    assert!(success.status.success(), "{success:?}");
    assert!(success.stderr.is_empty(), "{success:?}");
    assert!(String::from_utf8_lossy(&success.stdout).contains("renderer success: unchanged"));
    assert!(!notices.exists(), "an opened window sent a desktop notice");
    eprintln!("renderer startup success: unchanged");
    let output = command
        .env(CHILD, "1")
        .output()
        .expect("run the headless startup child");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let generic_count = stderr.matches(renderer::MESSAGE).count();
    let cause_count = stderr.lines().filter(|line| *line == CAUSE).count();
    eprintln!(
        "renderer startup observation: exit={:?} generic={generic_count} cause={cause_count}",
        output.status.code()
    );
    assert_eq!(
        output.status.code(),
        Some(1),
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
    assert_eq!(generic_count, 1, "{stderr}");
    assert!(!stderr.contains("renderer regression:"), "{stderr}");
    assert_eq!(
        cause_count, 1,
        "renderer startup cause missing or changed: {stderr}"
    );
    // The child's stderr is a pipe, as a launcher's is, so the same sentence reached the desktop once.
    let notice = std::fs::read_to_string(&notices).unwrap_or_default();
    assert_eq!(
        notice,
        "--app-name=Flea\n--urgency=critical\n--icon=com.thisisgm.flea\nNo graphics driver could open Flea\n\
         Install or repair vulkan-swrast and mesa, or run flea --tui in a terminal.\n",
        "{stderr}"
    );
}
