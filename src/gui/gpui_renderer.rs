use crate::vulkan;
use std::ffi::OsStr;
use std::process::Command;

pub(super) fn configure(cmd: &mut Command) {
    clear_qt_renderer(cmd);
    cmd.env_remove(vulkan::PIN_MARKER);
    let inherited = std::env::var_os(vulkan::PIN_MARKER).is_some_and(|value| !value.is_empty());
    if inherited {
        vulkan::drop_display_pin(cmd);
        // Probe the same unrestricted list the child will see, not the parent's stale display pin.
        std::env::remove_var("VK_DRIVER_FILES");
        std::env::remove_var("VK_ICD_FILENAMES");
        std::env::remove_var(vulkan::PIN_MARKER);
    }
    let idle = crate::icdexclude::exclude_absent_vendors();
    // GPUI reads compositor dmabuf/DRI3 feedback and tests adapters against its real surface.
    // A Qt connector pin would hide adapters from that selection and its GL fallback.
    if std::env::var_os("FLEA_GPU_REPORT").as_deref() == Some(OsStr::new("1")) {
        let filter = std::env::var("VK_LOADER_DRIVERS_DISABLE").unwrap_or_default();
        let operator = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"]
            .iter()
            .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()));
        eprintln!(
            "flea-gpu-policy: excluded={filter:?} operator_icd={operator} nvidia_idle_disabled={idle} selection=compositor-feedback"
        );
    }
}

fn clear_qt_renderer(cmd: &mut Command) {
    for key in [
        "QSG_RHI_BACKEND",
        "FLEA_RENDERER_AUTOMATIC",
        "QT_VK_PHYSICAL_DEVICE_INDEX",
    ] {
        cmd.env_remove(key);
    }
}

#[cfg(test)]
#[path = "gpui_renderer_nvidia_tests.rs"]
mod nvidia_tests;
