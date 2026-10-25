//! Child-process HOME isolation for direct cargo test runs.
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub(super) fn isolated() {
    use std::process::{Command, Stdio};
    let output = Command::new("mktemp")
        .args(["-d", "/tmp/flea-theme-gate-rust.XXXXXXXX"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with("/tmp") && root.components().count() == 3);
    std::fs::write(root.join(".flea-test-sandbox"), "theme gate\n").unwrap();
    for part in ["home", "config", "cache", "state", "runtime"] {
        std::fs::create_dir(root.join(part)).unwrap();
    }
    std::fs::set_permissions(root.join("runtime"), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(root.join("runtime/.flea-test-sandbox"), "private runtime\n").unwrap();
    std::fs::write(root.join("home/.flea-test-sandbox"), "private HOME\n").unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let binary = std::env::var_os("FLEA_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| target.join("debug/flea"));
    assert!(
        binary.is_file(),
        "build the backend first: tests/gpui-theme-gate.sh"
    );
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "--ignored",
            "theme_gate::live::theme_gate_live_switch_whole_window",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("FLEA_THEME_GATE_ROOT", &root)
        .env("FLEA_THEME_STOCK_DIR", super::stock_dir())
        .env("FLEA_BIN", binary)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    println!(
        "{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    // Only the fresh mktemp path carrying our marker can be removed.
    assert!(root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&root).unwrap();
    assert!(child.status.success(), "isolated live theme gate failed");
}
