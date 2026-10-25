use super::{
    apply_display_pin, apply_renderer, automatic_renderer, pin_display_icd, skip_gtk_platform_theme,
};
use crate::{prefetch, shelfplugin, thp, uistatecli, uistore};
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "gpui_renderer.rs"]
mod gpui_renderer;
#[path = "legacy.rs"]
mod legacy;
#[path = "legacy_command.rs"]
mod legacy_command;
#[path = "legacy_identity.rs"]
mod legacy_identity;

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
    cmd.env_remove("FLEA_PATH").env_remove("FLEA_SELECT");
    if let Some(target) = select {
        cmd.arg("--select").arg(target);
    }
    if let Some(path) = start {
        cmd.arg("--").arg(path);
    }
    cmd
}

pub(crate) fn settle_state() {
    // Both frontends settle state before the first paint; a refusal still opens on the original file.
    match uistore::Store::user().and_then(|store| store.settle()) {
        Ok(()) => {}
        Err(e) => eprintln!("flea: the view state was not settled ({})", e),
    }
    // Refresh an enabled shelf's installed plugin before either frontend opens.
    if let Ok(store) = uistore::Store::user() {
        if let Err(e) = shelfplugin::refresh(uistatecli::shelf_enabled(&store.read())) {
            eprintln!("flea: the shelf plugin was not refreshed ({})", e);
        }
    }
}

pub fn exec_gpui(start: Option<&str>, select: Option<&str>) -> i32 {
    exec_window(start, select, None)
}

pub fn exec_gpui_unlock(device: &str) -> i32 {
    exec_special(crate::unlock::ENV, device)
}

pub(crate) fn exec_special(key: &str, value: &str) -> i32 {
    exec_window(None, None, Some((key, value)))
}

const ENOENT: i32 = 2;
const EACCES: i32 = 13;

// The answer exec would give, checked before an old window is closed: ENOENT when absent, EACCES when not runnable.
fn binary_usable(binary: &Path) -> std::io::Result<()> {
    let usable = |path: &Path| {
        let meta = path.metadata()?;
        if meta.is_file() && meta.permissions().mode() & 0o111 != 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(EACCES))
        }
    };
    if binary.components().count() > 1 {
        return usable(binary);
    }
    let found = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| usable(&dir.join(binary)).is_ok())
    });
    if found {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(ENOENT))
    }
}

fn exec_window(start: Option<&str>, select: Option<&str>, special: Option<(&str, &str)>) -> i32 {
    let executable = std::env::current_exe().ok();
    let binary = gpui_binary(executable.as_deref(), |path| path.is_file());
    if let Err(error) = binary_usable(&binary) {
        eprintln!(
            "flea: the window could not start: {}: {error}",
            binary.display()
        );
        return 1;
    }
    // A picker or unlock launch is a dialog beside the operator's windows, never a 0.3.12 window's replacement.
    let carry = match special.map_or_else(legacy::replace, |_| Ok(Default::default())) {
        Ok(carry) => carry,
        Err(reason) => {
            eprintln!("flea: {reason}");
            return 2;
        }
    };
    let start = start.or_else(|| special.is_none().then_some(carry.path.as_deref()).flatten());
    let select = select.or_else(|| {
        (start == carry.path.as_deref())
            .then_some(carry.select.as_deref())
            .flatten()
    });
    settle_state();
    let mut cmd = gpui_argv(&binary, start, select);
    let selection = crate::jsondoc::Json::Arr(
        carry
            .selection
            .into_iter()
            .map(crate::jsondoc::Json::Str)
            .collect(),
    );
    cmd.env("FLEA_UPGRADE_SELECTION", crate::jsondoc::render(&selection));
    cmd.env(
        "FLEA_UPGRADE_WINDOWS",
        crate::jsondoc::render(&crate::jsondoc::Json::Arr(carry.windows)),
    );
    configure_gpui(&mut cmd);
    if let Some((key, value)) = special {
        cmd.env(key, value);
    }
    thp::disable();
    let error = cmd.exec();
    eprintln!(
        "flea: the window could not start: {}: {error}",
        binary.display()
    );
    1
}

fn gpui_pick_argv(binary: &Path, reply: &str) -> Command {
    let mut cmd = Command::new(binary);
    cmd.arg("--pick").arg(reply);
    cmd
}

// The chooser needs a graphical session, then runs on GPUI when chosen; None leaves it to Quickshell.
pub(super) fn chooser_frontend(reply: &str) -> Option<i32> {
    if !crate::paths::has_display() {
        eprintln!("flea: there is no graphical session to open a file chooser in");
        return Some(2);
    }
    use_gpui().then(|| exec_gpui_pick(reply))
}

// flea --pick under the GPUI frontend: FLEA_PICKER rides the environment, the reply path the argument.
fn exec_gpui_pick(reply: &str) -> i32 {
    let executable = std::env::current_exe().ok();
    let binary = gpui_binary(executable.as_deref(), |path| path.is_file());
    let mut cmd = gpui_pick_argv(&binary, reply);
    configure_gpui(&mut cmd);
    thp::disable();
    let error = cmd.exec();
    eprintln!("flea: the file chooser could not start: {}: {error}", binary.display());
    1
}

fn configure_gpui(cmd: &mut Command) {
    configure_child(cmd);
    set_backend_binary(cmd);
    gpui_renderer::configure(cmd);
}

fn configure_child(cmd: &mut Command) {
    // Only the main window records a prefetch list; a chooser started from a Flea terminal must not overwrite it.
    cmd.env_remove(prefetch::LIST_ENV);
    cmd.env_remove(prefetch::SHELL_ENV);
    // The gvfs head start belongs to the window that was opened on its path, never to a chooser.
    cmd.env_remove(crate::gvfsprefetch::PREFETCH_ENV);
    cmd.env_remove(crate::gvfsprefetch::PATH_ENV);
    cmd.env_remove(crate::gvfsprefetch::START_ENV);
    // A shell started under an --unlock window must not replay its dialog; unlock_command sets it back for the one launch.
    cmd.env_remove(crate::unlock::ENV);
}

fn set_backend_binary(cmd: &mut Command) {
    // An explicit choice is the operator's; map_or because is_none_or needs Rust 1.82 over the 1.77 floor.
    if std::env::var_os("FLEA_BIN").map_or(true, |value| value.is_empty()) {
        if let Ok(binary) = std::env::current_exe() {
            cmd.env("FLEA_BIN", binary);
        }
    }
}

pub(super) fn configure_shell(cmd: &mut Command) {
    configure_child(cmd);
    skip_gtk_platform_theme(cmd);
    set_backend_binary(cmd);
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
    fn a_gpui_pick_hands_the_reply_path_as_its_one_argument() {
        let cmd = gpui_pick_argv(Path::new("/usr/bin/flea-gui"), "/run/user/1/flea-picker-x/reply.json");
        assert_eq!(cmd.get_program(), "/usr/bin/flea-gui");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["--pick", "/run/user/1/flea-picker-x/reply.json"]);
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
    fn gpui_launch_environment_does_not_choose_a_qt_renderer() {
        let mut cmd = Command::new("flea-gui");
        configure_child(&mut cmd);
        set_backend_binary(&mut cmd);
        for key in [
            "QSG_RHI_BACKEND",
            "FLEA_RENDERER_AUTOMATIC",
            "QT_QPA_PLATFORMTHEME",
        ] {
            assert!(!cmd.get_envs().any(|(name, _)| name == key));
        }
    }

    #[test]
    fn gpui_carries_start_and_select_only_as_literal_arguments() {
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
        for key in ["FLEA_PATH", "FLEA_SELECT"] {
            assert!(cmd
                .get_envs()
                .any(|(name, value)| name == OsStr::new(key) && value.is_none()));
        }
        let bare = gpui_argv(Path::new("flea-gui"), None, None);
        assert_eq!(bare.get_args().count(), 0);
    }

    fn argv_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }
}
