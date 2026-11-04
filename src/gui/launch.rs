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

pub(crate) fn reject_legacy_frontend() -> bool {
    if std::env::var_os("FLEA_FRONTEND").as_deref() == Some(OsStr::new("qml")) {
        eprintln!("flea: the QML frontend is no longer available; unset FLEA_FRONTEND to open the GPUI window");
        true
    } else {
        false
    }
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
    // Settle state before the first paint; a refusal still opens on the original file.
    match uistore::Store::user().and_then(|store| store.settle()) {
        Ok(()) => {}
        Err(e) => eprintln!("flea: the view state was not settled ({})", e),
    }
    // Refresh an enabled shelf's installed plugin before the window opens.
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
    if reject_legacy_frontend() { return 2; }
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
    // Only the main window warms and records; a picker or unlock launch keeps the window's list.
    let warm = if special.is_none() {
        Warm::launch(start)
    } else {
        Warm::default()
    };
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
    warm.hand_to(&mut cmd, start);
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

// The page-cache list and the gvfs head start a main window launch hands its window; see AGENTS.md "The first window".
#[derive(Default)]
struct Warm {
    list: Option<PathBuf>,
    gvfs: Option<(PathBuf, u64)>,
}

impl Warm {
    // One helper fork for both jobs, before any other read this launch makes.
    fn launch(start: Option<&str>) -> Self {
        let list = prefetch::list_path();
        let prepared = start.and_then(crate::gvfsprefetch::prepare);
        let gvfs_arg = match (&prepared, start) {
            (Some((dest, _)), Some(path)) => Some((path, dest.as_path())),
            _ => None,
        };
        let warmed = crate::gui::warm_all(list.as_deref(), gvfs_arg);
        Warm {
            list,
            gvfs: prepared.filter(|_| warmed),
        }
    }

    fn hand_to(&self, cmd: &mut Command, start: Option<&str>) {
        if let Some(list) = &self.list {
            cmd.env(prefetch::LIST_ENV, list);
            // exec keeps this pid, so it is the window's own, and its backend records the window's pages.
            cmd.env(prefetch::SHELL_ENV, std::process::id().to_string());
        }
        if let Some((dest, start_ms)) = &self.gvfs {
            cmd.env(crate::gvfsprefetch::PREFETCH_ENV, dest);
            // The window's backend inherits this, so the first scan of this path can adopt the file.
            cmd.env(crate::gvfsprefetch::PATH_ENV, start.unwrap_or_default());
            cmd.env(crate::gvfsprefetch::START_ENV, start_ms.to_string());
        }
    }
}

fn gpui_pick_argv(binary: &Path, reply: &str) -> Command {
    let mut cmd = Command::new(binary);
    cmd.arg("--pick").arg(reply);
    cmd
}

// flea --pick under the GPUI frontend: FLEA_PICKER rides the environment, the reply path the argument.
pub(super) fn exec_gpui_pick(reply: &str) -> i32 {
    if reject_legacy_frontend() { return 2; }
    if !crate::paths::has_display() {
        eprintln!("flea: there is no graphical session to open a file chooser in");
        return 2;
    }
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
    flea::qscrash::restore(cmd);
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

#[cfg(test)]
mod tests {
    use super::*;

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
