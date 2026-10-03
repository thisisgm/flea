use crate::paths;
use crate::prefetch;
use crate::thp;
use crate::vulkan;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// std already links libc, so fork is declared here rather than taking a crate.
extern "C" {
    fn fork() -> i32;
}

// exec rather than spawn, so the shell replaces this process and no pid is orphaned.
pub fn exec_qs(ui: &Path, start: Option<&str>, select: Option<&str>) -> i32 {
    // Quickshell keeps dead entries forever, so Flea prunes only its own shells before qs starts.
    crate::qsregistry::prune_dead(&[crate::paths::SHELL_ID, crate::paths::PICKER_SHELL_ID]);
    // One helper fork for the launch: the page-cache list and the gvfs head start together.
    let list = prefetch::list_path();
    let prepared: Option<(PathBuf, u64)> = start.and_then(crate::gvfsprefetch::prepare);
    let gvfs_arg: Option<(&str, &Path)> = match (&prepared, start) {
        (Some((dest, _)), Some(path)) => Some((path, dest.as_path())),
        _ => None,
    };
    let warmed = warm_all(list.as_deref(), gvfs_arg);
    let gvfs: Option<(PathBuf, u64)> = prepared.filter(|_| warmed);
    let mut cmd = qs_command(ui.join(paths::ENTRY));
    if let Some(list) = &list {
        cmd.env(prefetch::LIST_ENV, list);
        // exec keeps this pid, so it is the shell's own.
        cmd.env(prefetch::SHELL_ENV, std::process::id().to_string());
    }
    if let Some((dest, start_ms)) = &gvfs {
        cmd.env(crate::gvfsprefetch::PREFETCH_ENV, dest);
        // The backend Process inherits this, so the first scan of this path can adopt the file.
        cmd.env(crate::gvfsprefetch::PATH_ENV, start.unwrap_or_default());
        cmd.env(crate::gvfsprefetch::START_ENV, start_ms.to_string());
    }
    if let Some(path) = start {
        cmd.env("FLEA_PATH", path);
    }
    if let Some(target) = select {
        cmd.env("FLEA_SELECT", target);
    }
    exec(cmd)
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
            cmd.arg(path);
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
fn warm_all(list: Option<&Path>, gvfs: Option<(&str, &Path)>) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(mut cmd) = launch_warm_command(&exe, list, gvfs) else { return false };
    // The child forks and its parent exits at once, so this wait is only the fork.
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
    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a file chooser in");
        return 2;
    }
    let Some(ui) = paths::ui_dir() else {
        eprintln!("{}", paths::missing_ui_message());
        return 2;
    };
    // Quickshell keeps dead entries forever, so Flea prunes only its own shells before qs starts.
    crate::qsregistry::prune_dead(&[crate::paths::SHELL_ID, crate::paths::PICKER_SHELL_ID]);
    let mut cmd = qs_command(ui.join(paths::PICKER_ENTRY));
    cmd.env("FLEA_PICKER_REPLY", reply);
    exec(cmd)
}

// What the automatic arm hands qs: Vulkan, the hasvk fallback, or the unusable-loader downgrade.
enum LaunchRenderer {
    Vulkan(Vec<(u32, u32, u32)>),
    HasvkFallback,
    UnusableFallback(String),
}

// Sample input: Ok(vec![(0x8086, 0x0412, 1)]) for a Haswell-only box, Err for no Vulkan at all.
fn automatic_renderer(probe: Result<Vec<(u32, u32, u32)>, String>) -> LaunchRenderer {
    match probe {
        Err(reason) => LaunchRenderer::UnusableFallback(reason),
        Ok(devices) if vulkan::hasvk_only(&devices) => LaunchRenderer::HasvkFallback,
        Ok(devices) => LaunchRenderer::Vulkan(devices),
    }
}

// The env the automatic arm hands qs, split so a test can read it back through override_of.
fn apply_renderer(cmd: &mut Command, renderer: LaunchRenderer) {
    match renderer {
        LaunchRenderer::UnusableFallback(reason) => {
            // A silent downgrade hides a 2.4x memory regression, so the reason the probe found is said once.
            eprintln!("flea: Vulkan is unusable, {reason}, so the shell starts on OpenGL");
            cmd.env("QSG_RHI_BACKEND", "opengl");
            cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
        }
        LaunchRenderer::HasvkFallback => {
            // Issue #160: hasvk draws garbled text, so the fallback names the driver and retries nothing.
            eprintln!("flea: the only Vulkan GPU is hasvk (Intel Ivy Bridge to Broadwell), which draws garbled text, so the shell starts on OpenGL");
            cmd.env("QSG_RHI_BACKEND", "opengl");
            cmd.env_remove("FLEA_RENDERER_AUTOMATIC");
        }
        LaunchRenderer::Vulkan(devices) => {
            // Vulkan is the measured fast path, and the marker is what permits the QML arm its one retry.
            cmd.env("QSG_RHI_BACKEND", "vulkan");
            cmd.env("FLEA_RENDERER_AUTOMATIC", "1");
            pin_display_icd(cmd, Some(&devices));
        }
    }
}

// The qs invocation both entry points share, including the renderer chosen at the last hand-off point.
fn qs_command(target: PathBuf) -> Command {
    let mut cmd = Command::new("qs");
    cmd.arg("-p").arg(target);
    // Only the main window records a prefetch list; a chooser started from a Flea terminal must not overwrite it.
    cmd.env_remove(prefetch::LIST_ENV);
    cmd.env_remove(prefetch::SHELL_ENV);
    // The gvfs head start belongs to the window that was opened on its path, never to a chooser.
    cmd.env_remove(crate::gvfsprefetch::PREFETCH_ENV);
    cmd.env_remove(crate::gvfsprefetch::PATH_ENV);
    cmd.env_remove(crate::gvfsprefetch::START_ENV);
    skip_gtk_platform_theme(&mut cmd);
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
            pin_display_icd(&mut cmd, None);
        }
    } else {
        apply_renderer(&mut cmd, automatic_renderer(vulkan::usable()));
    }
    cmd
}

// `already` carries the automatic arm's probe result, so a hybrid launch does not pay for Vulkan twice.
fn pin_display_icd(cmd: &mut Command, already: Option<&[(u32, u32, u32)]>) {
    if operator_chose_icd(
        std::env::var_os("VK_DRIVER_FILES").as_deref(),
        std::env::var_os("VK_ICD_FILENAMES").as_deref(),
    ) {
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
    apply_display_pin(cmd, vulkan::display_pin(devices));
}

// Split from pin_display_icd so a test can drive the guard without touching this process's environment.
fn operator_chose_icd(driver: Option<&OsStr>, icd: Option<&OsStr>) -> bool {
    driver.is_some_and(|value| !value.is_empty()) || icd.is_some_and(|value| !value.is_empty())
}

// Split from pin_display_icd so a test can drive the answer a single-GPU box can never produce.
fn apply_display_pin(cmd: &mut Command, pin: vulkan::DisplayPin) {
    match pin {
        vulkan::DisplayPin::NotNeeded => {}
        vulkan::DisplayPin::Unmatched { vendor } => {
            eprintln!("flea: Vulkan sees a GPU with no display, but no ICD file names vendor {vendor:#06x}, so the loader's own list stands");
        }
        vulkan::DisplayPin::Pin { icd, gpu } => {
            eprintln!("flea: Vulkan sees a GPU with no display, so the shell starts on {:#06x}:{:#06x} through {icd}", gpu.0, gpu.1);
            cmd.env("VK_DRIVER_FILES", &icd);
            cmd.env("VK_ICD_FILENAMES", &icd);
            cmd.env(vulkan::PIN_MARKER, "1");
        }
    }
}

// Omarchy's own platform theme, and the only one this trades away, see AGENTS.md "The first window".
const GTK3: &str = "gtk3";

// The launcher marks the theme it traded, so a program Flea opens gets it back.
pub const THEME_MARKER: &str = "FLEA_QT_THEME";

// gtk3 as Qt's platform theme starts GTK inside the shell for one string, the icon theme name.
fn skip_gtk_platform_theme(cmd: &mut Command) {
    // Empty is absent, the rule paths::has_display() applies: a wrapper's unset variable is not a choice.
    if std::env::var_os("QS_ICON_THEME").is_some_and(|value| !value.is_empty()) {
        return;
    }
    // Any other engine is the operator's own choice and is never traded.
    if std::env::var_os("QT_QPA_PLATFORMTHEME").as_deref() != Some(OsStr::new(GTK3)) {
        return;
    }
    let Some(home) = std::env::var_os("HOME") else { return };
    let path = PathBuf::from(home).join(".local/state/omarchy/current/theme/icons.theme");
    // Sample input, the whole file: "Yaru-purple\n"
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let name = text.trim();
    if name.is_empty() {
        return;
    }
    cmd.env("QS_ICON_THEME", name);
    cmd.env_remove("QT_QPA_PLATFORMTHEME");
    cmd.env(THEME_MARKER, GTK3);
}

// Give a program Flea opens the platform theme this launcher traded away, and nothing else.
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

fn exec(mut cmd: Command) -> i32 {
    // The setting is preserved across exec, so this is the last point that can hand it to qs.
    thp::disable();
    // exec() only returns on failure; the reason is elided, never shown raw.
    let _ = cmd.exec();
    eprintln!("flea: could not start the shell, qs is not on PATH or failed to run");
    1
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

    #[test]
    fn a_display_gpu_pin_sets_both_loader_variables() {
        let icd = String::from("/usr/share/vulkan/icd.d/nvidia_icd.json");
        let mut cmd = Command::new("true");
        apply_display_pin(&mut cmd, vulkan::DisplayPin::Pin { icd: icd.clone(), gpu: (0x10de, 0x27e0) });
        assert_eq!(override_of(&cmd, "VK_DRIVER_FILES").as_deref(), Some(icd.as_str()));
        assert_eq!(override_of(&cmd, "VK_ICD_FILENAMES").as_deref(), Some(icd.as_str()));
    }

    #[test]
    fn a_pin_marks_itself_so_a_child_can_tell_it_from_the_operators_own_list() {
        let mut cmd = Command::new("true");
        apply_display_pin(&mut cmd, vulkan::DisplayPin::Pin { icd: String::from("/x.json"), gpu: (0x10de, 0x27e0) });
        assert_eq!(override_of(&cmd, vulkan::PIN_MARKER).as_deref(), Some("1"));
    }

    #[test]
    fn an_exported_but_empty_loader_variable_is_absent_not_a_choice() {
        assert!(!operator_chose_icd(None, None));
        assert!(!operator_chose_icd(Some(OsStr::new("")), Some(OsStr::new(""))));
        assert!(operator_chose_icd(Some(OsStr::new("/a.json")), None));
        assert!(operator_chose_icd(None, Some(OsStr::new("/b.json"))));
    }

    #[test]
    fn a_box_needing_no_pin_sets_no_loader_variable() {
        let mut cmd = Command::new("true");
        apply_display_pin(&mut cmd, vulkan::DisplayPin::NotNeeded);
        assert_eq!(override_of(&cmd, "VK_DRIVER_FILES"), None);
        assert_eq!(override_of(&cmd, "VK_ICD_FILENAMES"), None);
        assert_eq!(override_of(&cmd, vulkan::PIN_MARKER), None);
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

    #[test]
    fn a_display_gpu_with_no_icd_file_sets_no_loader_variable() {
        let mut cmd = Command::new("true");
        apply_display_pin(&mut cmd, vulkan::DisplayPin::Unmatched { vendor: 0x10de });
        assert_eq!(override_of(&cmd, "VK_DRIVER_FILES"), None);
        assert_eq!(override_of(&cmd, "VK_ICD_FILENAMES"), None);
    }

    // Issue #160: the only usable device is hasvk, so the launch is OpenGL with no retry left.
    #[test]
    fn a_hasvk_only_probe_starts_on_opengl_with_no_retry() {
        match automatic_renderer(Ok(vec![(0x8086, 0x0412, 1)])) {
            LaunchRenderer::HasvkFallback => {}
            _ => panic!("a Haswell-only probe must fall back"),
        }
        let mut cmd = Command::new("true");
        apply_renderer(&mut cmd, automatic_renderer(Ok(vec![(0x8086, 0x0412, 1)])));
        assert_eq!(override_of(&cmd, "QSG_RHI_BACKEND").as_deref(), Some("opengl"));
        assert!(removed(&cmd, "FLEA_RENDERER_AUTOMATIC"));
    }

    // hasvk beside another usable device keeps Vulkan, with the retry the automatic arm permits.
    #[test]
    fn a_probe_beside_hasvk_starts_on_vulkan_with_retry() {
        for probe in [vec![(0x8086, 0x0412, 1), (0x10de, 0x27e0, 2)], vec![(0x8086, 0xa788, 1)]] {
            assert!(matches!(automatic_renderer(Ok(probe.clone())), LaunchRenderer::Vulkan(_)));
            let mut cmd = Command::new("true");
            apply_renderer(&mut cmd, automatic_renderer(Ok(probe)));
            assert_eq!(override_of(&cmd, "QSG_RHI_BACKEND").as_deref(), Some("vulkan"));
            assert_eq!(override_of(&cmd, "FLEA_RENDERER_AUTOMATIC").as_deref(), Some("1"));
        }
    }

    // No Vulkan at all is the existing downgrade, unchanged by the hasvk arm beside it.
    #[test]
    fn no_vulkan_at_all_is_the_existing_downgrade() {
        match automatic_renderer(Err(String::from("vkCreateInstance answered -9"))) {
            LaunchRenderer::UnusableFallback(reason) => assert!(reason.contains("vkCreateInstance"), "{reason}"),
            _ => panic!("an unusable loader must downgrade"),
        }
        let mut cmd = Command::new("true");
        apply_renderer(&mut cmd, automatic_renderer(Err(String::from("vkCreateInstance answered -9"))));
        assert_eq!(override_of(&cmd, "QSG_RHI_BACKEND").as_deref(), Some("opengl"));
        assert!(removed(&cmd, "FLEA_RENDERER_AUTOMATIC"));
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
