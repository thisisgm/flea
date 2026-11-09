use crate::prefetch;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

mod launch;
pub use launch::{exec_gpui, exec_gpui_unlock};
pub(crate) use launch::reject_legacy_frontend;

// std already links libc, so fork is declared here rather than taking a crate.
extern "C" {
    fn fork() -> i32;
}

// One helper invocation for the launch, so a gvfs start pays one fork, never two.
pub(crate) fn launch_warm_command(exe: &Path, list: Option<&Path>, gvfs: Option<(&str, &Path)>) -> Option<Command> {
    let has_list = list.is_some_and(|p| p.is_file());
    if !has_list && gvfs.is_none() {
        return None;
    }
    let mut cmd = Command::new(exe);
    cmd.arg("--launch-warm");
    // Sample args: "flea --launch-warm /cache/flea/prefetch /run/user/1000/gvfs/x/dir /run/user/1000/flea/gvfs-1.list"; "-" skips a job.
    cmd.arg(if has_list { list.unwrap().as_os_str() } else { OsStr::new("-") });
    match gvfs {
        Some((path, dest)) => {
            cmd.arg(flea::bytename::text_name(path));
            cmd.arg(dest);
        }
        None => {
            cmd.arg("-");
            cmd.arg("-");
        }
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // The gvfs head start outlives the exec, the page-cache job sharing its fork; a local launch keeps today's shape.
    if gvfs.is_some() {
        cmd.process_group(0);
    }
    Some(cmd)
}

// The launcher's side: one spawn and one fork-wait for both jobs, true when the helper started.
pub(crate) fn warm_all(list: Option<&Path>, gvfs: Option<(&str, &Path)>) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(mut cmd) = launch_warm_command(&exe, list, gvfs) else { return false };
    match cmd.spawn() {
        Ok(mut child) => {
            let _ = child.wait();
            true
        }
        Err(_) => false,
    }
}

// flea --launch-warm <list> <gvfs-path> <gvfs-dest>: one fork for both jobs; "-" skips a job.
pub fn run_launch_warm(list: &str, gvfs_path: &str, gvfs_dest: &str) -> i32 {
    // corner: a fork that fails keeps the work here, and the launcher waits it out, which is still a launch.
    if unsafe { fork() } > 0 {
        return 0;
    }
    let gio = crate::backend::gvfslist::gio_bin();
    let owned = list.to_owned();
    run_warm_jobs(list, gvfs_path, gvfs_dest, &gio, crate::gvfsprefetch::runtime_dir().as_deref(), move || {
        prefetch::do_warm(Path::new(&owned))
    })
}

// Both jobs at once: gio runs here while the warm runs on a thread, then both join.
fn run_warm_jobs(list: &str, gvfs_path: &str, gvfs_dest: &str, gio: &str, runtime: Option<&Path>, warm: impl FnOnce() + Send + 'static) -> i32 {
    let warm = (list != "-").then(|| std::thread::spawn(warm));
    if gvfs_path != "-" && gvfs_dest != "-" {
        let _ = crate::gvfsprefetch::run_in(gvfs_path, Path::new(gvfs_dest), gio, runtime);
    }
    if let Some(warm) = warm {
        let _ = warm.join();
    }
    0
}

// flea --pick <reply>: the portal's chooser window on the same shell and renderer choice.
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
    launch::exec_gpui_pick(reply)
}

// Old windows may pass this marker through an upgrade.
pub const THEME_MARKER: &str = "FLEA_QT_THEME";

// Give a program Flea opens the platform theme an old window's launcher traded away, and nothing else.
pub fn restore_platform_theme(command: &mut Command) {
    apply_theme_restore(command, std::env::var_os(THEME_MARKER).as_deref());
}

// Split from restore_platform_theme so a test can drive it without this process's environment.
fn apply_theme_restore(command: &mut Command, marker: Option<&OsStr>) {
    let Some(theme) = marker.filter(|value| !value.is_empty()) else {
        return;
    };
    command.env("QT_QPA_PLATFORMTHEME", theme);
    command.env_remove("QS_ICON_THEME");
    command.env_remove(THEME_MARKER);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Command keeps its overrides rather than a rendered environment, so this reads back what was set.
    fn override_of(cmd: &Command, key: &str) -> Option<String> {
        cmd.get_envs()
            .find(|(name, _)| *name == OsStr::new(key))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }

    // A removal is a present key with no value, which override_of cannot tell from an absent one.
    fn removed(cmd: &Command, key: &str) -> bool {
        cmd.get_envs().any(|(name, value)| name == OsStr::new(key) && value.is_none())
    }

    #[test]
    fn a_traded_platform_theme_is_handed_back_to_an_opened_program() {
        let mut cmd = Command::new("true");
        apply_theme_restore(&mut cmd, Some(OsStr::new("gtk3")));
        assert_eq!(override_of(&cmd, "QT_QPA_PLATFORMTHEME").as_deref(), Some("gtk3"));
        assert!(removed(&cmd, "QS_ICON_THEME"));
        assert!(removed(&cmd, THEME_MARKER));
    }

    #[test]
    fn an_unmarked_launch_leaves_an_opened_programs_theme_alone() {
        let mut cmd = Command::new("true");
        apply_theme_restore(&mut cmd, None);
        assert_eq!(cmd.get_envs().count(), 0);
    }

    #[test]
    fn an_exported_but_empty_theme_marker_is_absent_not_a_trade() {
        let mut cmd = Command::new("true");
        apply_theme_restore(&mut cmd, Some(OsStr::new("")));
        assert_eq!(cmd.get_envs().count(), 0);
    }

    // The argv a launch helper carries, read back from the built command.
    fn argv_of(cmd: &Command) -> Vec<String> {
        cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn a_gvfs_launch_carries_both_jobs_in_one_helper_command() {
        let dir = crate::backend::testdir::TestDir::new("launch-warm-gvfs");
        let list = dir.file("prefetch", "flea-prefetch 2\n");
        let dest = dir.join("gvfs-1.list");
        let share = "/run/user/1000/gvfs/smb-share:server=x,share=y/dir";
        let cmd = launch_warm_command(Path::new("flea"), Some(list.as_path()), Some((share, dest.as_path())))
            .expect("a gvfs launch with a list warms");
        assert_eq!(argv_of(&cmd), vec!["--launch-warm", list.to_str().unwrap(), share, dest.to_str().unwrap()]);
    }

    #[test]
    fn a_local_launch_carries_no_gvfs_job() {
        let dir = crate::backend::testdir::TestDir::new("launch-warm-local");
        let list = dir.file("prefetch", "flea-prefetch 2\n");
        let cmd = launch_warm_command(Path::new("flea"), Some(list.as_path()), None).expect("a local launch with a list warms");
        assert_eq!(argv_of(&cmd), vec!["--launch-warm", list.to_str().unwrap(), "-", "-"]);
    }

    #[test]
    fn a_launch_warm_runs_the_warm_beside_gio_and_waits_for_both() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let dir = crate::backend::testdir::TestDir::new("launch-warm-both");
        let list = dir.file("prefetch", "flea-prefetch 2\n");
        let runtime = dir.path().join("runtime/flea");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        let started = dir.join("warm-started");
        // The fake gio waits, bounded, for the warm to start, so the dest proves they ran together.
        let gio = dir.script("gio", &format!("#!/bin/sh\nfor i in $(seq 1 100); do [ -e '{}' ] && break; sleep 0.02; done\n[ -e '{}' ] || exit 1\nprintf 'smb://h/share/a.txt\\t3\\t(regular)\\ttime::modified=100\\n'\n", started.to_string_lossy(), started.to_string_lossy())).to_string_lossy().into_owned();
        let dest = runtime.join("gvfs-1.list");
        let share = "/run/user/1000/gvfs/smb-share:server=t,share=u/dir";
        let started_job = Arc::new(AtomicBool::new(false));
        let finished_job = Arc::new(AtomicBool::new(false));
        let started_flag = Arc::clone(&started_job);
        let finished_flag = Arc::clone(&finished_job);
        let dest_job = dest.clone();
        assert_eq!(run_warm_jobs(list.to_str().unwrap(), share, dest.to_str().unwrap(), &gio, Some(&runtime), move || {
            std::fs::write(&started, "started").unwrap();
            started_flag.store(true, Ordering::Relaxed);
            // Bounded wait for gio's answer, so only the join proves the warm finished.
            for _ in 0..50 {
                if dest_job.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if dest_job.exists() {
                finished_flag.store(true, Ordering::Relaxed);
            }
        }), 0);
        assert!(dest.is_file(), "both jobs are done when the helper returns");
        let body = std::fs::read_to_string(&dest).unwrap();
        assert!(body.contains("smb://h/share/a.txt"), "gio's listing landed, not a failure marker: {body}");
        assert!(started_job.load(Ordering::Relaxed), "the warm ran beside gio, which waited for its start");
        assert!(finished_job.load(Ordering::Relaxed), "and the warm finished before the helper returned");
    }

    #[test]
    fn a_launch_with_nothing_to_warm_spawns_nothing() {
        let dir = crate::backend::testdir::TestDir::new("launch-warm-none");
        let missing = dir.join("no-such-list");
        assert!(launch_warm_command(Path::new("flea"), Some(missing.as_path()), None).is_none());
        assert!(launch_warm_command(Path::new("flea"), None, None).is_none());
    }
}
