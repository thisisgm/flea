use crate::paths;
use crate::thp;
use crate::vulkan;
use std::ffi::OsStr;
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
        // Vulkan on a hybrid GPU still has to present on the compositor's device; pinning is not a renderer change.
        if std::env::var_os("QSG_RHI_BACKEND").as_deref() == Some(OsStr::new("vulkan")) {
            pin_display_icd(&mut cmd, None);
        }
    } else {
        match vulkan::usable() {
            Err(reason) => {
                // A silent downgrade hides a 2.4x memory regression, so the reason the probe found is said once.
                eprintln!("flea: Vulkan is unusable, {reason}, so the shell starts on OpenGL");
                cmd.env("QSG_RHI_BACKEND", "opengl");
                cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
            }
            Ok(devices) => {
                // Vulkan is the measured fast path, and the marker is what permits the QML arm its one retry.
                cmd.env("QSG_RHI_BACKEND", "vulkan");
                cmd.env("FLEA_RENDERER_AUTOMATIC", "1");
                pin_display_icd(&mut cmd, Some(&devices));
            }
        }
    }
    cmd
}

// VK_DRIVER_FILES / VK_ICD_FILENAMES are the loader's; an explicit value is the operator's.
// Empty is absent, the same rule QSG_RHI_BACKEND follows. The probe is reused when the automatic
// arm already ran it, so a hybrid launch does not pay for Vulkan twice.
fn pin_display_icd(cmd: &mut Command, already: Option<&[(u32, u32)]>) {
    if std::env::var_os("VK_DRIVER_FILES").is_some_and(|value| !value.is_empty())
        || std::env::var_os("VK_ICD_FILENAMES").is_some_and(|value| !value.is_empty())
    {
        return;
    }
    let owned;
    let devices = match already {
        Some(devices) => devices,
        None => match vulkan::usable() {
            Ok(devices) => {
                owned = devices;
                owned.as_slice()
            }
            Err(_) => return,
        },
    };
    if let Some(icd) = vulkan::display_icd(devices) {
        eprintln!("flea: Vulkan sees a GPU with no display, so the shell starts on the display GPU");
        cmd.env("VK_DRIVER_FILES", &icd);
        cmd.env("VK_ICD_FILENAMES", &icd);
    }
}

fn exec(mut cmd: Command) -> i32 {
    // The setting is preserved across exec, so this is the last point that can hand it to qs.
    thp::disable();
    // exec() only returns on failure; the reason is elided, never shown raw.
    let _ = cmd.exec();
    eprintln!("flea: could not start the shell, qs is not on PATH or failed to run");
    1
}
