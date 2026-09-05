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
    // Vulkan is Flea's measured fast path, and the probe is what keeps a loader that cannot deliver
    // it out of QRhi::create, which crashes there instead of raising the error the QML arm listens for.
    // Empty is absent, the same rule paths::has_display() applies: an unset variable in a wrapper
    // script exports as empty, and that is an accident rather than the operator naming a renderer.
    if std::env::var_os("QSG_RHI_BACKEND").is_some_and(|value| !value.is_empty()) {
        // An explicit choice is the operator's, so it is neither replaced nor offered a retry.
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
    } else if vulkan::usable() {
        cmd.env("QSG_RHI_BACKEND", "vulkan");
        // The marker is what permits the QML arm its one retry, and only this implicit choice gets it.
        cmd.env("FLEA_RENDERER_AUTOMATIC", "1");
    } else {
        // OpenGL is where that retry would have gone, so there is nothing left for it to mark.
        cmd.env("QSG_RHI_BACKEND", "opengl");
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
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
