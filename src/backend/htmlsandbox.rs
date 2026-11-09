use flea::bytename::ExactText as _;
use std::path::Path;
use super::sandbox::{BWRAP, BWRAP_FLAGS, CPU_SECONDS, PRLIMIT};

// Chromium reserves large virtual ranges, so this still-image helper keeps the CPU cap without an address-space cap.
pub fn wrap_html(inner: &[String], page: &Path, writable: &Path) -> Vec<String> {
    let folder = page.parent().unwrap_or(Path::new("/"));
    let mut argv = vec![
        PRLIMIT.to_string(),
        format!("--cpu={CPU_SECONDS}"),
        BWRAP.to_string(),
    ];
    argv.extend(BWRAP_FLAGS.iter().map(|flag| flag.to_string()));
    for (flag, from, to) in [
        ("--ro-bind", folder, "/page"),
        ("--bind", writable, "/output"),
    ] {
        argv.extend([
            flag.to_string(),
            from.exact_text().into_owned(),
            to.to_string(),
        ]);
    }
    let confined = writable.join("input.html");
    let destination = Path::new("/page").join(page.file_name().unwrap_or_default());
    argv.extend([
        "--ro-bind".to_string(),
        confined.exact_text().into_owned(),
        destination.exact_text().into_owned(),
    ]);
    for (name, value) in [
        ("QT_QPA_PLATFORM", "offscreen"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("HOME", "/tmp"),
        ("QT_FORCE_STDERR_LOGGING", "1"),
        ("QTWEBENGINE_REMOTE_DEBUGGING", "127.0.0.1:9222"),
    ] {
        argv.extend(["--setenv".to_string(), name.to_string(), value.to_string()]);
    }
    argv.extend_from_slice(inner);
    argv
}
