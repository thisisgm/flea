use crate::thp;
use crate::vulkan;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

// exec rather than spawn, so the shell replaces this process and no pid is orphaned.
pub fn exec_qs(ui: &Path, start: Option<&str>, select: Option<&str>) -> i32 {
    let mut cmd = Command::new("qs");
    cmd.arg("-p").arg(ui);
    if let Ok(binary) = std::env::current_exe() {
        cmd.env("FLEA_BIN", binary);
    }
    // Empty is absent, the rule paths::has_display() applies: a wrapper's unset variable is not a choice.
    if std::env::var_os("QSG_RHI_BACKEND").is_some_and(|value| !value.is_empty()) {
        // An explicit choice is the operator's, so it is neither replaced nor offered a retry.
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
    } else if let Err(reason) = vulkan::usable() {
        // A silent downgrade hides a 2.4x memory regression, so the reason the probe found is said once.
        eprintln!("flea: Vulkan is unusable, {reason}, so the shell starts on OpenGL");
        cmd.env("QSG_RHI_BACKEND", "opengl");
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
    } else {
        // Vulkan is the measured fast path, and the marker is what permits the QML arm its one retry.
        cmd.env("QSG_RHI_BACKEND", "vulkan");
        cmd.env("FLEA_RENDERER_AUTOMATIC", "1");
    }
    if let Some(path) = start {
        cmd.env("FLEA_PATH", path);
    }
    if let Some(target) = select {
        cmd.env("FLEA_SELECT", target);
    }
    // The setting is preserved across exec, so this is the last point that can hand it to qs.
    thp::disable();
    // exec() only returns on failure; the reason is elided, never shown raw.
    let _ = cmd.exec();
    eprintln!("flea: could not start the shell, qs is not on PATH or failed to run");
    1
}
