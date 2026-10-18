use super::{apply_renderer, automatic_renderer, pin_display_icd, skip_gtk_platform_theme};
use crate::{prefetch, thp};
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn use_gpui() -> bool {
    frontend_is_gpui(std::env::var_os("FLEA_FRONTEND").as_deref())
}

fn frontend_is_gpui(frontend: Option<&OsStr>) -> bool {
    frontend == Some(OsStr::new("gpui"))
}

fn gpui_binary(executable: Option<&Path>, is_file: impl Fn(&Path) -> bool) -> PathBuf {
    if let Some(parent) = executable.and_then(Path::parent) {
        let sibling = parent.join("flea-gui");
        if is_file(&sibling) {
            return sibling;
        }
    }
    PathBuf::from("flea-gui")
}

fn gpui_argv(binary: &Path, start: Option<&str>, select: Option<&str>) -> Command {
    let mut cmd = Command::new(binary);
    if let Some(target) = select {
        cmd.arg("--select").arg(target).env("FLEA_SELECT", target);
    }
    if let Some(path) = start {
        cmd.arg("--").arg(path).env("FLEA_PATH", path);
    }
    cmd
}

pub fn exec_gpui(start: Option<&str>, select: Option<&str>) -> i32 {
    let executable = std::env::current_exe().ok();
    let binary = gpui_binary(executable.as_deref(), |path| path.is_file());
    let mut cmd = gpui_argv(&binary, start, select);
    configure_shell(&mut cmd);
    // wgpu uses Vulkan regardless of the Qt renderer setting.
    if gpui_needs_probe(std::env::var_os("QSG_RHI_BACKEND").as_deref()) {
        pin_display_icd(&mut cmd, None);
    }
    thp::disable();
    let _ = cmd.exec();
    eprintln!("flea: the window could not start: flea-gui is not installed or could not run");
    1
}

fn gpui_needs_probe(renderer: Option<&OsStr>) -> bool {
    renderer.is_some_and(|value| !value.is_empty() && value != OsStr::new("vulkan"))
}

pub(super) fn configure_shell(cmd: &mut Command) {
    // Only the main window records a prefetch list; a chooser started from a Flea terminal must not overwrite it.
    cmd.env_remove(prefetch::LIST_ENV);
    cmd.env_remove(prefetch::SHELL_ENV);
    // The gvfs head start belongs to the window that was opened on its path, never to a chooser.
    cmd.env_remove(crate::gvfsprefetch::PREFETCH_ENV);
    cmd.env_remove(crate::gvfsprefetch::PATH_ENV);
    cmd.env_remove(crate::gvfsprefetch::START_ENV);
    // A shell started under an --unlock window must not replay its dialog; unlock_command sets it back for the one launch.
    cmd.env_remove(crate::unlock::ENV);
    skip_gtk_platform_theme(cmd);
    // An explicit choice is the operator's; map_or because is_none_or needs Rust 1.82 over the 1.77 floor.
    if std::env::var_os("FLEA_BIN").map_or(true, |value| value.is_empty()) {
        if let Ok(binary) = std::env::current_exe() {
            cmd.env("FLEA_BIN", binary);
        }
    }
    // Before the probe and qs in every arm, so neither loads a driver for a GPU vendor this box does not hold.
    crate::icdexclude::exclude_absent_vendors();
    // Empty is absent, the rule paths::has_display() applies: a wrapper's unset variable is not a choice.
    if std::env::var_os("QSG_RHI_BACKEND").is_some_and(|value| !value.is_empty()) {
        // An explicit choice is the operator's, so it is neither replaced nor offered a retry.
        cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
        // Vulkan on a hybrid GPU still has to present on the compositor's device; pinning is not a renderer change.
        if std::env::var_os("QSG_RHI_BACKEND").as_deref() == Some(OsStr::new("vulkan")) {
            pin_display_icd(cmd, None);
        }
    } else {
        apply_renderer(cmd, automatic_renderer(crate::vkprobecache::usable()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_developer_switch_selects_gpui() {
        for value in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("qml")),
            Some(OsStr::new("GPUI")),
        ] {
            assert!(!frontend_is_gpui(value));
        }
        assert!(frontend_is_gpui(Some(OsStr::new("gpui"))));
    }

    #[test]
    fn gpui_prefers_the_sibling_binary_then_path() {
        let executable = Some(Path::new("/opt/flea/bin/flea"));
        assert_eq!(
            gpui_binary(executable, |path| path
                == Path::new("/opt/flea/bin/flea-gui")),
            Path::new("/opt/flea/bin/flea-gui")
        );
        assert_eq!(gpui_binary(executable, |_| false), Path::new("flea-gui"));
        assert_eq!(gpui_binary(None, |_| true), Path::new("flea-gui"));
    }

    #[test]
    fn gpui_probes_an_explicit_qt_gl_choice_without_repeating_the_vulkan_pin() {
        assert!(gpui_needs_probe(Some(OsStr::new("opengl"))));
        assert!(!gpui_needs_probe(Some(OsStr::new("vulkan"))));
        assert!(!gpui_needs_probe(Some(OsStr::new(""))));
        assert!(!gpui_needs_probe(None));
    }

    #[test]
    fn gpui_carries_start_and_select_as_literal_arguments_and_environment() {
        let cmd = gpui_argv(
            Path::new("/opt/flea-gui"),
            Some("/files/a b"),
            Some("/files/a b/\"selected\""),
        );
        assert_eq!(cmd.get_program(), OsStr::new("/opt/flea-gui"));
        assert_eq!(
            argv_of(&cmd),
            ["--select", "/files/a b/\"selected\"", "--", "/files/a b"]
        );
        assert_eq!(
            override_of(&cmd, "FLEA_PATH").as_deref(),
            Some("/files/a b")
        );
        assert_eq!(
            override_of(&cmd, "FLEA_SELECT").as_deref(),
            Some("/files/a b/\"selected\"")
        );
        let bare = gpui_argv(Path::new("flea-gui"), None, None);
        assert_eq!(bare.get_args().count(), 0);
    }

    fn argv_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    fn override_of(command: &Command, key: &str) -> Option<String> {
        command
            .get_envs()
            .find(|(name, _)| *name == OsStr::new(key))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }
}
