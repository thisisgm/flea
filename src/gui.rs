use crate::paths;
use crate::thp;
use crate::vulkan;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

// exec rather than spawn, so the shell replaces this process and no pid is orphaned.
pub fn exec_qs(ui: &Path, start: Option<&str>, select: Option<&str>) -> i32 {
    let mut cmd = qs_command(ui.to_path_buf());
    if let Some(path) = start {
        cmd.env("FLEA_PATH", path);
    }
    if let Some(target) = select {
        cmd.env("FLEA_SELECT", target);
    }
    exec(cmd)
}

// flea --pick <reply>: the picker window tools/flea-portal opens for one portal request. Same shell
// and the same renderer choice, on a second entry point, so a chooser is not a second application.
pub fn pick(reply: &str) -> i32 {
    // Empty is absent, the rule paths::has_display() applies: a wrapper's unset variable is not a request.
    if !std::env::var_os("FLEA_PICKER").is_some_and(|value| !value.is_empty()) {
        eprintln!("flea: --pick needs FLEA_PICKER, the portal request tools/flea-portal puts in the environment");
        return 2;
    }
    if reply.is_empty() {
        eprintln!("flea: --pick needs the reply file tools/flea-portal names, and it was empty");
        return 2;
    }
    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a file chooser in");
        return 2;
    }
    let Some(ui) = paths::ui_dir() else {
        eprintln!("flea: the shell config is missing, set FLEA_UI or install /usr/share/flea/ui");
        return 2;
    };
    let mut cmd = qs_command(ui.join("picker.qml"));
    cmd.env("FLEA_PICKER_REPLY", reply);
    exec(cmd)
}

// The qs invocation both entry points share: the target, the binary the shell calls back into, and
// the renderer, which is chosen here because this is the last point that can hand it to qs.
fn qs_command(target: PathBuf) -> Command {
    let mut cmd = Command::new("qs");
    cmd.arg("-p").arg(target);
    // An explicit choice is the operator's, the same rule FLEA_UI and QSG_RHI_BACKEND follow here.
    // map_or, not is_none_or: that method landed in 1.82 and Cargo.toml declares a 1.77 floor.
    if std::env::var_os("FLEA_BIN").map_or(true, |value| value.is_empty()) {
        if let Ok(binary) = std::env::current_exe() {
            cmd.env("FLEA_BIN", binary);
        }
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
    cmd
}

fn exec(mut cmd: Command) -> i32 {
    // The setting is preserved across exec, so this is the last point that can hand it to qs.
    thp::disable();
    // exec() only returns on failure; the reason is elided, never shown raw.
    let _ = cmd.exec();
    eprintln!("flea: could not start the shell, qs is not on PATH or failed to run");
    1
}
