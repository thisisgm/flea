//! Backend records pass through the binary's real startup logger and stderr in an isolated child.
use std::process::Command;

const CHILD: &str = "FLEA_S6_BACKEND_LOG_CHILD";
const TEST: &str = "backend_log_tests::s6_backend_log_reports_selection_without_info_flood";
const RENDERER: &str = "gpui_wgpu::wgpu_renderer";
const CONTEXT: &str = "gpui_wgpu::wgpu_context";

pub(super) fn emit_if_requested() -> bool {
    let Ok(case) = std::env::var(CHILD) else {
        return false;
    };
    log::info!(target: "wgpu_core", "unrelated driver info");
    log::info!(target: CONTEXT, "unrelated context info");
    log::warn!(target: RENDERER, "unrelated renderer warning");
    if case == "fallback" || case == "two_windows" {
        log::warn!(target: RENDERER, "No usable Backends(VULKAN) adapter for this window: Failed to create surface: no Vulkan driver\n  loader found no ICD");
    }
    if case == "fallback" {
        log::info!(target: CONTEXT, "  Adapter {:?} (Vulkan) failed: Failed to create wgpu device: Parent device is lost, trying next...", "Test GPU");
    }
    if case == "refused" {
        log::info!(target: CONTEXT, "  Adapter {:?} (Gl) failed: Failed to create wgpu device: Limit\n  'max_storage_buffer_binding_size' value 134217728 is better than allowed 0, trying next...", "Test GPU");
    } else if case == "vulkan" {
        log::info!(target: CONTEXT, "Selected GPU adapter: {:?} (Vulkan)", "Test GPU");
    } else {
        log::info!(target: CONTEXT, "Selected GPU adapter: {:?} (Gl)", "Test GPU");
    }
    if case == "two_windows" {
        log::info!(target: CONTEXT, "Selected GPU adapter: {:?} (Vulkan)", "Second GPU");
    }
    // A selection drops the refusals before it, so only a start that opened no window prints any here.
    for refusal in crate::backend_log::take_refusals() {
        eprintln!("{refusal}");
    }
    true
}

#[test]
fn s6_backend_log_reports_selection_without_info_flood() {
    if std::env::var_os(CHILD).is_some() {
        super::main();
        return;
    }
    let fallback = "flea: Vulkan is unusable, Failed to create surface: no Vulkan driver loader found no ICD, so the window starts on OpenGL (\"Test GPU\")\n";
    for (case, expected) in [
        (
            "vulkan",
            "flea: the window starts on Vulkan (\"Test GPU\")\n".to_owned(),
        ),
        (
            "gl",
            "flea: the window starts on OpenGL (\"Test GPU\")\n".to_owned(),
        ),
        ("fallback", fallback.to_owned()),
        (
            "two_windows",
            format!("{fallback}flea: the window starts on Vulkan (\"Second GPU\")\n"),
        ),
        (
            "refused",
            "\"Test GPU\" (Gl) was refused: Failed to create wgpu device: Limit 'max_storage_buffer_binding_size' value 134217728 is better than allowed 0\n".to_owned(),
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, case)
            .output()
            .unwrap();
        assert!(output.status.success(), "{case}: {output:?}");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            expected,
            "{case}"
        );
    }
}
