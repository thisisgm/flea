// A selection is validated whole before the first handler starts; GIO still owns desktop-entry expansion.
use crate::{backend::menu_registry, desktopid, imageopen::Budget, runfile, runid};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Group {
    application: PathBuf,
    paths: Vec<PathBuf>,
}

// Only approved refusal keys cross to the existing footer; an unknown cause keeps the ordinary opener failure.
enum Refusal {
    Folder,
    Archive,
    Program,
    Failed,
}

impl Refusal {
    fn report(self) -> i32 {
        let key = match self {
            Self::Folder => Some("feedback.refusal.folder_with_files"),
            Self::Archive => Some("feedback.refusal.archives"),
            Self::Program => Some("feedback.refusal.programs"),
            Self::Failed => None,
        };
        if let Some(key) = key {
            println!(r#"{{"refusal":"{key}"}}"#);
        }
        eprintln!("flea: no application on this system opened that file");
        super::FAILED
    }
}

pub(super) fn open(paths: &[String]) -> i32 {
    let groups = match prepare(paths) {
        Ok(groups) => groups,
        Err(refusal) => return refusal.report(),
    };
    for group in groups {
        let mut launcher = Command::new("gio");
        launcher
            .arg("launch")
            .arg(group.application)
            .args(group.paths);
        if !super::handoff(launcher).is_ok_and(|status| status.success()) {
            return Refusal::Failed.report();
        }
    }
    0
}

fn prepare(paths: &[String]) -> Result<Vec<Group>, Refusal> {
    if paths.is_empty() || paths.len() > crate::imageopen::MAX_IMAGES {
        return Err(Refusal::Failed);
    }
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mut budget = Budget::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut defaults = HashMap::new();
    let mut indices = HashMap::new();
    let mut identities = Vec::with_capacity(paths.len());
    for path in paths {
        let target = std::fs::canonicalize(path).map_err(|_| Refusal::Failed)?;
        if !budget.take(&target) {
            return Err(Refusal::Failed);
        }
        if target.is_dir() {
            return Err(Refusal::Folder);
        }
        let opened = runid::snapshot(&target).ok_or(Refusal::Failed)?;
        match runfile::decide_opened(&opened, &target, &mounts) {
            runfile::Decision::OpenNormally
            | runfile::Decision::MakeExecutable(runfile::Kind::Script) => {}
            _ => return Err(Refusal::Program),
        }
        if archive(Path::new(path)) || archive(&target) {
            return Err(Refusal::Archive);
        }
        let mime = menu_registry::content_type(&Default::default(), &target, &Default::default())
            .map_err(|_| Refusal::Failed)?;
        let application = if let Some(application) = defaults.get(&mime) {
            PathBuf::clone(application)
        } else {
            let id = desktopid::default_handler_gio(&mime);
            let application = desktopid::resolve(&id).ok_or(Refusal::Failed)?;
            menu_registry::desktop_exec_raw(&application)
                .map_err(|_| Refusal::Failed)?
                .filter(|exec| !exec.trim().is_empty())
                .ok_or(Refusal::Failed)?;
            defaults.insert(mime, application.clone());
            application
        };
        let index = *indices.entry(application.clone()).or_insert_with(|| {
            groups.push(Group {
                application,
                paths: Vec::new(),
            });
            groups.len() - 1
        });
        groups[index].paths.push(target.clone());
        identities.push((target, opened.id));
    }
    // A registry lookup can take time; recheck the entire plan before any handler sees it.
    for (target, identity) in identities {
        let current = runid::snapshot(&target).ok_or(Refusal::Failed)?;
        if !identity.matches(&current.id) {
            return Err(Refusal::Failed);
        }
        match runfile::decide_opened(&current, &target, &mounts) {
            runfile::Decision::OpenNormally
            | runfile::Decision::MakeExecutable(runfile::Kind::Script) => {}
            _ => return Err(Refusal::Program),
        }
    }
    Ok(groups)
}

fn archive(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    !name.to_ascii_lowercase().ends_with(".appimage")
        && flea::ui_logic_pv1::kinds::is_archive(&name)
}
