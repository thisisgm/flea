// A deliberate multi-selection is one launch per default application, not one gio open per file.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::os::unix::process::CommandExt;

fn ordinary(target: &Path, mounts: &str) -> bool {
    let Some(opened) = crate::runid::snapshot(target) else { return false };
    matches!(crate::runfile::decide_opened(&opened, target, mounts),
        crate::runfile::Decision::OpenNormally |
        crate::runfile::Decision::MakeExecutable(crate::runfile::Kind::Script))
}

fn group(groups: &mut Vec<(PathBuf, Vec<PathBuf>)>, entry: PathBuf, target: PathBuf) {
    if let Some((_, paths)) = groups.iter_mut().find(|(app, _)| *app == entry) {
        paths.push(target);
    } else {
        groups.push((entry, vec![target]));
    }
}

fn launch(entry: &Path, paths: &[PathBuf]) -> Command {
    let mut child = Command::new("gio");
    child.arg("launch").arg(entry).args(paths)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .process_group(0);
    child
}

pub fn dispatch(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some("--open-many") { return None }
    if args.len() < 4 {
        eprintln!("flea: --open-many takes at least two paths");
        return Some(crate::open::FAILED);
    }
    Some(open(&args[2..]))
}

fn open(paths: &[String]) -> i32 {
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mut groups = Vec::new();
    // Validate the whole selection before launching anything. Programs still require the single-file run card.
    for path in paths {
        let target = match std::fs::canonicalize(path) {
            Ok(target) if target.is_file() && ordinary(&target, &mounts) => target,
            _ => {
                eprintln!("flea: open files individually when the selection contains folders, programs or unavailable files");
                return crate::open::FAILED;
            }
        };
        let mime = match crate::backend::menu_registry::content_type(&Default::default(), &target, &Default::default()) {
            Ok(mime) => mime,
            Err(_) => return failed(),
        };
        let id = crate::desktopid::default_handler_gio(&mime);
        let Some(entry) = crate::desktopid::resolve(&id) else { return failed() };
        group(&mut groups, entry, target);
    }
    let mut result = 0;
    for (entry, paths) in groups {
        let mut child = launch(&entry, &paths);
        crate::open::prepare(&mut child);
        if !child.status().is_ok_and(|status| status.success()) {
            result = failed();
        }
    }
    result
}

fn failed() -> i32 {
    eprintln!("flea: no application on this system opened the selected files");
    crate::open::FAILED
}

#[cfg(test)]
#[path = "open_many_tests.rs"]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_mode_requires_a_multi_selection() {
        assert_eq!(dispatch(&["flea".into(), "--open-many".into()]), Some(crate::open::FAILED));
        assert_eq!(dispatch(&["flea".into(), "--open-many".into(), "/one.png".into()]), Some(crate::open::FAILED));
        assert_eq!(dispatch(&["flea".into(), "--open".into(), "/one.png".into()]), None);
    }

    #[test]
    fn same_handler_keeps_mixed_image_types_in_selection_order() {
        let mut groups = Vec::new();
        group(&mut groups, "/apps/imv.desktop".into(), "/pics/a.png".into());
        group(&mut groups, "/apps/editor.desktop".into(), "/pics/notes.txt".into());
        group(&mut groups, "/apps/imv.desktop".into(), "/pics/b.jpg".into());
        assert_eq!(groups, vec![
            (PathBuf::from("/apps/imv.desktop"), vec![PathBuf::from("/pics/a.png"), PathBuf::from("/pics/b.jpg")]),
            (PathBuf::from("/apps/editor.desktop"), vec![PathBuf::from("/pics/notes.txt")]),
        ]);
    }

    #[test]
    fn batch_launch_passes_every_path_as_a_separate_argument() {
        let paths = vec![PathBuf::from("/pics/a b.png"), PathBuf::from("/pics/--flag;'$.jpg")];
        let child = launch(Path::new("/apps/imv.desktop"), &paths);
        assert_eq!(child.get_program(), "gio");
        let args: Vec<_> = child.get_args().collect();
        assert_eq!(args, [std::ffi::OsStr::new("launch"), std::ffi::OsStr::new("/apps/imv.desktop"), paths[0].as_os_str(), paths[1].as_os_str()]);
    }
}
