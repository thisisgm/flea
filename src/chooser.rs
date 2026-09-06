// flea --picker: the per-user step that routes the desktop's file chooser here, see docs/install.md.
use crate::hyprkeys;
use crate::userfile::{config_home, create_file, env_dir, home, replace_file};
use std::fs;
use std::path::PathBuf;

// The interface Flea's backend implements, and the only key in portals.conf that is Flea's to write.
const IFACE: &str = "org.freedesktop.impl.portal.FileChooser";
// gtk stays behind flea, so a box whose flea.portal went missing still has a chooser at all.
const PREFERRED: &str = "flea;gtk";
// What tools/flea-portal registers as; xdg-desktop-portal names a backend by this file's stem.
const PORTAL_FILE: &str = "flea.portal";
const GROUP: &str = "[preferred]";

// flea --picker
// --default asks this before claiming, because a box with no flea.portal has nothing to prefer.
pub fn backend_installed() -> bool {
    installed_portal().is_some()
}

pub fn claim() -> i32 {
    if installed_portal().is_none() {
        eprintln!(
            "flea: {} is not installed in any portal directory, so there is no backend to prefer; install the package first",
            PORTAL_FILE
        );
        return 1;
    }
    report(claim_chooser(), hyprkeys::float_claim())
}

// flea --picker off
pub fn release() -> i32 {
    report(release_chooser(), hyprkeys::float_release())
}

// Each half stands on its own, so a failure in one still leaves the other's line on screen.
fn report(routing: Result<String, String>, window: Result<String, String>) -> i32 {
    let mut status = 0;
    for half in [routing, window] {
        match half {
            Ok(line) => println!("{}", line),
            Err(why) => {
                eprintln!("flea: {}", why);
                status = 1;
            }
        }
    }
    // The portal reads its configuration once, at startup, so a live session keeps the old routing.
    println!("xdg-desktop-portal reads this at startup: systemctl --user restart xdg-desktop-portal");
    status
}

fn claim_chooser() -> Result<String, String> {
    let path = conf_path()?;
    if let Some(shadow) = shadowing_file()? {
        return Err(format!(
            "{} would never be read, because {} is desktop specific and wins inside the same directory; add {}={} to that file instead",
            path.display(),
            shadow.display(),
            IFACE,
            PREFERRED
        ));
    }
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let dir = path.parent().ok_or_else(|| format!("{} has no directory", path.display()))?;
            fs::create_dir_all(dir).map_err(|e| format!("{} could not be created ({:?})", dir.display(), e.kind()))?;
            create_file(&path, &format!("{}\n{}={}\n", GROUP, IFACE, PREFERRED))?;
            return Ok(format!("{}: {}, written to {}", IFACE, PREFERRED, path.display()));
        }
        Err(e) => return Err(format!("{} could not be read ({:?})", path.display(), e.kind())),
    };
    let Some(next) = set_preferred(&text, IFACE, PREFERRED) else {
        return Ok(format!("{}: already {} in {}", IFACE, PREFERRED, path.display()));
    };
    replace_file(&path, &next)?;
    Ok(format!(
        "{}: {}, written to {}; every other interface keeps the routing it had",
        IFACE,
        PREFERRED,
        path.display()
    ))
}

fn release_chooser() -> Result<String, String> {
    let path = conf_path()?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("{}: nothing to undo, {} does not exist", IFACE, path.display()));
        }
        Err(e) => return Err(format!("{} could not be read ({:?})", path.display(), e.kind())),
    };
    let Some(next) = drop_preferred(&text, IFACE) else {
        return Ok(format!("{}: nothing to undo, {} does not name it", IFACE, path.display()));
    };
    // A file left holding nothing but the group heading was this command's own, so it goes with the key.
    if next.trim() == GROUP {
        fs::remove_file(&path).map_err(|e| format!("{} could not be removed ({:?})", path.display(), e.kind()))?;
        return Ok(format!("{}: Flea's line removed, and {} held nothing else, so it is gone", IFACE, path.display()));
    }
    replace_file(&path, &next)?;
    Ok(format!("{}: Flea's line removed from {}", IFACE, path.display()))
}

fn conf_path() -> Result<PathBuf, String> {
    Ok(config_home()?.join("xdg-desktop-portal").join("portals.conf"))
}

// Inside one directory xdg-desktop-portal reads <desktop>-portals.conf and stops, so a file for this
// desktop hides the plain one entirely. Sample input: XDG_CURRENT_DESKTOP=Hyprland gives hyprland-portals.conf.
fn shadowing_file() -> Result<Option<PathBuf>, String> {
    let dir = config_home()?.join("xdg-desktop-portal");
    let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    for name in desktops.split(':').filter(|d| !d.is_empty()) {
        let candidate = dir.join(format!("{}-portals.conf", name.to_lowercase()));
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

// The lookup xdg-desktop-portal 1.22 makes for a backend: the data home first, then every data dir,
// then its own datadir. Verified against src/xdp-portal-config.c load_installed_portals() at tag 1.22.1.
fn installed_portal() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    match env_dir("XDG_DATA_HOME") {
        Some(p) => dirs.push(p),
        None => {
            if let Ok(h) = home() {
                dirs.push(h.join(".local/share"));
            }
        }
    }
    let system = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty());
    let system = system.unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.push(PathBuf::from("/usr/share"));
    dirs.into_iter()
        .map(|d| d.join("xdg-desktop-portal").join("portals").join(PORTAL_FILE))
        .find(|p| p.is_file())
}

// portals.conf(5) is a key file, of which only one key in one group is Flea's:
//   [preferred]
//   default=hyprland;gtk
//   org.freedesktop.impl.portal.FileChooser=flea;gtk
// Returns the file with `key=value` in [preferred], or None when it already says exactly that. A
// default= line is never touched: it is what every other interface still resolves through.
pub fn set_preferred(text: &str, key: &str, value: &str) -> Option<String> {
    let line = format!("{}={}\n", key, value);
    if !text.contains(GROUP) {
        let mut out = text.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(GROUP);
        out.push('\n');
        out.push_str(&line);
        return Some(out);
    }
    let mut out = String::with_capacity(text.len() + line.len());
    let mut in_group = false;
    let mut written = false;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\n', '\r']);
        if body.trim_start().starts_with('[') {
            // Leaving the group without having found the key: it goes in at the end of the group.
            if in_group && !written {
                out.push_str(&line);
                written = true;
            }
            in_group = body.trim() == GROUP;
        } else if in_group && !written {
            if let Some(had) = body.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')) {
                if had.trim() == value {
                    return None;
                }
                out.push_str(&line);
                written = true;
                continue;
            }
        }
        out.push_str(raw);
    }
    if !written {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&line);
    }
    Some(out)
}

// Returns the file without `key` in [preferred], or None when that group does not name it.
pub fn drop_preferred(text: &str, key: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut in_group = false;
    let mut changed = false;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\n', '\r']);
        if body.trim_start().starts_with('[') {
            in_group = body.trim() == GROUP;
        } else if in_group && body.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')).is_some() {
            changed = true;
            continue;
        }
        out.push_str(raw);
    }
    if changed {
        Some(out)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX_SHAPE: &str = "[preferred]\ndefault=hyprland;gtk\n";

    #[test]
    fn set_preferred_adds_the_key_without_touching_the_default() {
        let out = set_preferred(BOX_SHAPE, IFACE, PREFERRED).expect("the file gains a line");
        assert_eq!(out, "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n");
    }

    #[test]
    fn set_preferred_creates_the_group_when_the_file_has_another_one() {
        let out = set_preferred("[something]\nkey=value\n", IFACE, PREFERRED).expect("the file gains a group");
        assert_eq!(out, "[something]\nkey=value\n[preferred]\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n");
    }

    #[test]
    fn set_preferred_replaces_another_backend_and_answers_none_for_its_own() {
        let held = "[preferred]\norg.freedesktop.impl.portal.FileChooser=gtk\ndefault=hyprland\n";
        let out = set_preferred(held, IFACE, PREFERRED).expect("the value changes");
        assert_eq!(out, "[preferred]\norg.freedesktop.impl.portal.FileChooser=flea;gtk\ndefault=hyprland\n");
        assert_eq!(set_preferred(&out, IFACE, PREFERRED), None);
    }

    #[test]
    fn set_preferred_puts_the_key_inside_the_group_and_not_after_the_next_one() {
        let two = "[preferred]\ndefault=hyprland\n\n[other]\nkey=value\n";
        let out = set_preferred(two, IFACE, PREFERRED).expect("the file gains a line");
        assert_eq!(out, "[preferred]\ndefault=hyprland\n\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n[other]\nkey=value\n");
    }

    #[test]
    fn drop_preferred_removes_only_fleas_line() {
        let held = "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n";
        assert_eq!(drop_preferred(held, IFACE), Some("[preferred]\ndefault=hyprland;gtk\n".to_string()));
        assert_eq!(drop_preferred(BOX_SHAPE, IFACE), None);
        assert_eq!(drop_preferred("", IFACE), None);
    }

    #[test]
    fn drop_preferred_leaves_the_same_key_in_another_group_alone() {
        let elsewhere = "[other]\norg.freedesktop.impl.portal.FileChooser=gtk\n";
        assert_eq!(drop_preferred(elsewhere, IFACE), None);
    }
}
