//! Preflight a marked selection completely, then hand each default application every file it opens.
use super::{prepare, resolved, FAILED};
use crate::{backend::menu_registry, runfile, runid};
use flea::ui_logic_fb1::OpenRefusal;
use std::os::unix::process::CommandExt;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

struct Group {
    app: PathBuf,
    paths: Vec<PathBuf>,
    // An Exec that takes a list gets the group in one launch; one that takes a path is launched once per file.
    list: bool,
}

struct Plan {
    groups: Vec<Group>,
    identities: Vec<(PathBuf, runid::FileId)>,
}

fn plan(paths: &[String]) -> Result<Plan, Option<OpenRefusal>> {
    if paths.is_empty() {
        return Err(Some(OpenRefusal::NotLoaded));
    }
    let registry = menu_registry::Registry::default();
    let icons = crate::backend::icons::Names::load();
    let cancel = Default::default();
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mut result = Plan {
        groups: Vec::new(),
        identities: Vec::new(),
    };
    let mut budget = crate::imageopen::Budget::new();
    let mut applications = std::collections::HashMap::new();
    for path in paths {
        let target = resolved(path).ok_or(Some(OpenRefusal::NotLoaded))?;
        let meta = target
            .metadata()
            .map_err(|_| Some(OpenRefusal::NotLoaded))?;
        if meta.is_dir() {
            return Err(Some(OpenRefusal::FolderWithFiles));
        }
        if !meta.is_file() || !budget.take(&target) {
            return Err(Some(OpenRefusal::NotLoaded));
        }
        let opened = runid::snapshot(&target).ok_or(Some(OpenRefusal::NotLoaded))?;
        match runfile::decide_opened(&opened, &target, &mounts) {
            runfile::Decision::OpenNormally
            | runfile::Decision::MakeExecutable(runfile::Kind::Script) => {}
            _ => return Err(Some(OpenRefusal::Programs)),
        }
        let name = target.to_string_lossy();
        if flea::ui_logic_pv1::kinds::quick_look_kind("", &name, false)
            == flea::ui_logic_pv1::kinds::State::Archive
        {
            return Err(Some(OpenRefusal::Archives));
        }
        let mime = menu_registry::content_type(&registry, &target, &cancel).map_err(|_| None)?;
        if flea::ui_logic_pv1::kinds::kind_state(icons.icon_for(Some(&mime), false, opened.mode))
            == flea::ui_logic_pv1::kinds::State::Archive
        {
            return Err(Some(OpenRefusal::Archives));
        }
        let app = match applications.get(&mime) {
            Some(path) => PathBuf::clone(path),
            None => {
                let app = menu_registry::handlers(&registry, &mime, &cancel)
                    .map_err(|_| None)?
                    .into_iter()
                    .find(|app| app.default)
                    .ok_or(None)?
                    .path;
                applications.insert(mime, app.clone());
                app
            }
        };
        let index = result.groups.iter().position(|group| group.app == app);
        let index = index.unwrap_or_else(|| {
            result.groups.push(Group {
                app,
                paths: Vec::new(),
                list: true,
            });
            result.groups.len() - 1
        });
        result.identities.push((target.clone(), opened.id));
        result.groups[index].paths.push(target);
    }
    for group in &mut result.groups {
        if group.paths.len() > 1 {
            let exec = menu_registry::desktop_exec_raw(&group.app)
                .map_err(|_| None)?
                .ok_or(None)?;
            group.list = crate::imageopen::takes_many(&exec);
        }
    }
    if result
        .identities
        .iter()
        .any(|(path, id)| runid::capture(path).as_ref() != Some(id))
    {
        return Err(Some(OpenRefusal::NotLoaded));
    }
    Ok(result)
}

pub fn open_selected(paths: &[String]) -> i32 {
    let plan = match plan(paths) {
        Ok(plan) => plan,
        Err(reason) => {
            if let Some(reason) = reason {
                println!(r#"{{"refusal":"{}"}}"#, reason.key());
            }
            return FAILED;
        }
    };
    for group in plan.groups {
        let batches: Vec<Vec<PathBuf>> = if group.list {
            vec![group.paths]
        } else {
            group.paths.into_iter().map(|path| vec![path]).collect()
        };
        for batch in batches {
            if !launch(&group.app, batch) {
                return FAILED;
            }
        }
    }
    0
}

fn launch(app: &Path, paths: Vec<PathBuf>) -> bool {
    let mut launch = Command::new("gio");
    launch.arg("launch").arg(app).args(paths);
    prepare(&mut launch);
    launch
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .status()
        .is_ok_and(|status| status.success())
}
