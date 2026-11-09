// Open With over several selected files: the applications every selected type names, and the launches one choice makes.
use flea::bytename::ExactText as _;
use super::menu_actions::{application_entries, Selected};
use super::menu_registry::{self, Application, Registry};
use super::trashmanifest::Cancellation;
use crate::json::{escape, field_str};
use std::ffi::OsString;
use std::path::Path;

// One gio info names the type of each file it is given; a bounded batch keeps its argument list short.
const BATCH: usize = 256;

pub(super) fn perform(
    op: &str,
    items: &[Selected],
    line: &str,
    registry: &Registry,
    cancel: &Cancellation,
) -> Result<String, String> {
    let paths: Vec<&Path> = items.iter().map(|item| item.path.as_path()).collect();
    if op == "applications" {
        let apps = catalogue(registry, &paths, cancel)?;
        return Ok(format!(
            r#""applications":[{}],"installed":[]"#,
            application_entries(&apps)
        ));
    }
    let app = menu_registry::resolve(&field_str(line, "application").unwrap_or_default(), cancel)?;
    let current = |range: std::ops::Range<usize>| {
        items[range]
            .iter()
            .try_for_each(|item| item.current().map(|_| ()))
    };
    launch(registry, &app.path, &paths, current, cancel)?;
    let named = paths
        .iter()
        .map(|path| format!(r#""{}""#, escape(&path.exact_text())))
        .collect::<Vec<_>>();
    Ok(format!(r#""paths":[{}]"#, named.join(",")))
}

// The flyout's applications: those the registry names for every selected type, in the first type's order.
fn catalogue(
    registry: &Registry,
    paths: &[&Path],
    cancel: &Cancellation,
) -> Result<Vec<Application>, String> {
    let mut types: Vec<String> = Vec::new();
    for batch in paths.chunks(BATCH) {
        let mut args: Vec<&std::ffi::OsStr> = vec![
            "info".as_ref(),
            "--attributes=standard::content-type".as_ref(),
        ];
        args.extend(batch.iter().map(|path| path.as_os_str()));
        for mime in content_types(&registry.query(&args, cancel)?, batch.len())? {
            if !types.contains(&mime) {
                types.push(mime);
            }
        }
    }
    let lists = types
        .iter()
        .map(|mime| menu_registry::handlers(registry, mime, cancel))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(intersect(lists))
}

// Sample gio info block per file: "uri: file:///a.png" ... "  standard::content-type: image/png"; a count that differs is refused.
fn content_types(info: &str, count: usize) -> Result<Vec<String>, String> {
    let types: Vec<String> = info
        .lines()
        .filter_map(|line| line.strip_prefix("  standard::content-type: "))
        .filter(|mime| !mime.is_empty())
        .map(str::to_string)
        .collect();
    if types.len() != count {
        return Err("The selected item's file type is unknown.".into());
    }
    Ok(types)
}

// An application stays when every list names it, and stays the default only where every type has it as the default.
fn intersect(lists: Vec<Vec<Application>>) -> Vec<Application> {
    let mut lists = lists.into_iter();
    let Some(mut common) = lists.next() else {
        return Vec::new();
    };
    for list in lists {
        common.retain_mut(|app| match list.iter().find(|other| other.id == app.id) {
            Some(other) => {
                app.default &= other.default;
                true
            }
            None => false,
        });
    }
    common
}

// Every launch the entry's Exec asks for, each after the selection is checked to be the one the menu snapshotted.
fn launch(
    registry: &Registry,
    desktop: &Path,
    paths: &[&Path],
    current: impl Fn(std::ops::Range<usize>) -> Result<(), String>,
    cancel: &Cancellation,
) -> Result<(), String> {
    let exec = menu_registry::desktop_key(desktop, "Exec=")?.unwrap_or_default();
    let mut first = 0;
    for group in groups(&exec, paths) {
        current(first..first + group.len())?;
        registry.launch(desktop, &group, cancel)?;
        first += group.len();
    }
    Ok(())
}

// An entry whose Exec takes a list (%F or %U) launches once with every file; any other launches once per file.
// Sample Exec values: "imv %F", "editor %f", "printf 100%%F %f".
fn groups<'a>(exec: &str, paths: &[&'a Path]) -> Vec<Vec<&'a Path>> {
    let mut codes = exec.split('%').skip(1);
    let mut list = false;
    while let Some(after) = codes.next() {
        // "%%" is a literal percent sign, so the piece after it is not a field code.
        if after.is_empty() {
            codes.next();
            continue;
        }
        list |= after.starts_with('F') || after.starts_with('U');
    }
    if list {
        vec![paths.to_vec()]
    } else {
        paths.iter().map(|path| vec![*path]).collect()
    }
}

pub(super) fn launch_args(desktop: &Path, paths: &[&Path]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["launch".into(), desktop.into()];
    args.extend(paths.iter().map(|path| path.as_os_str().to_owned()));
    args
}

// The launch failure names its one file, or how many it was given.
pub(super) fn named(paths: &[&Path]) -> String {
    match paths {
        [one] => one.display().to_string(),
        _ => format!("{} items", paths.len()),
    }
}

#[cfg(test)]
#[path = "menu_many_tests.rs"]
mod tests;
