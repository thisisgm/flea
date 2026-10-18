// flea --picker: the per-user step that routes the desktop's file chooser here, see docs/install.md.
use crate::error::io_message;
use crate::hyprkeys;
use crate::userfile::{config_home, create_file, data_file, replace_file};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[path = "chooser_startup.rs"]
mod startup;

// The interface Flea's backend implements, and the only key in portals.conf that is Flea's to write.
const IFACE: &str = "org.freedesktop.impl.portal.FileChooser";
// gtk stays behind flea, so a box whose flea.portal went missing still has a chooser at all.
const PREFERRED: &str = "flea;gtk";
// What tools/flea-portal registers as; xdg-desktop-portal names a backend by this file's stem.
const PORTAL_FILE: &str = "flea.portal";
const GROUP: &str = "[preferred]";
// The comment Flea leaves above its line when it replaced another backend, so `off` can put that one back.
// Sample line: # flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk
const REPLACED: &str = "# flea replaced: ";
// A desktop-specific configuration is <desktop>-portals.conf; the plain portals.conf has no dash before it.
const DESKTOP_SUFFIX: &str = "-portals.conf";
const RESTARTED: &str = "xdg-desktop-portal restarted if it was running, so file dialogs follow now";
const AT_STARTUP: &str = "xdg-desktop-portal reads this at startup: systemctl --user restart xdg-desktop-portal";

// flea --picker
// --default asks this before claiming, because a box with no flea.portal has nothing to prefer.
pub fn backend_installed() -> bool {
    installed_portal().is_some()
}

// `command` is the one the user ran, so a refusal that asks for a rerun names the right one.
pub fn claim(command: &str) -> i32 {
    if installed_portal().is_none() {
        eprintln!(
            "flea: {} is not installed in any portal folder, so the file chooser was not changed. Install the package first.",
            PORTAL_FILE
        );
        return 1;
    }
    // A portal file that cannot be named is refused before either half, so a refused claim writes nothing.
    let target = match effective_conf(command) {
        Ok(target) => target,
        Err(why) => {
            eprintln!("flea: {}", why);
            return 1;
        }
    };
    report(claim_chooser(target), hyprkeys::float_claim())
}

// flea --picker off
pub fn release() -> i32 {
    report(release_chooser(), hyprkeys::float_release())
}

// Each half stands on its own, so a failure in one still leaves the other's line on screen.
fn report(routing: Result<String, String>, window: Result<String, String>) -> i32 {
    let routed = routing.is_ok();
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
    // The portal reads its configuration once, at startup; a refused or failed routing left nothing for it to follow.
    if routed {
        println!("{}", follow_line(std::io::stdout().is_terminal(), restart_portal));
    }
    status
}

// Output on a terminal gets the restart done; piped or redirected output is Settings > About or a script, which own theirs.
fn follow_line(terminal: bool, restart: impl FnOnce() -> bool) -> &'static str {
    if terminal && restart() {
        RESTARTED
    } else {
        AT_STARTUP
    }
}

// The frontend must answer too: a successful systemctl exit alone is not a working chooser.
fn restart_portal() -> bool {
    startup::restart()
}

fn claim_chooser((path, desktop_file): (PathBuf, bool)) -> Result<String, String> {
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let dir = path.parent().ok_or_else(|| format!("{} has no parent folder", path.display()))?;
            fs::create_dir_all(dir).map_err(|e| format!("{} could not be created: {}", dir.display(), io_message(&e)))?;
            create_file(&path, &format!("{}\n{}={}\n", GROUP, IFACE, PREFERRED))?;
            return Ok(format!("{}: {}, written to {}", IFACE, PREFERRED, path.display()));
        }
        Err(e) => return Err(format!("{} could not be read: {}", path.display(), io_message(&e))),
    };
    let before = preferred_value(&text, IFACE);
    let Some(next) = set_preferred(&text, IFACE, PREFERRED) else {
        return Ok(format!("{}: already {} in {}", IFACE, PREFERRED, path.display()));
    };
    replace_file(&path, &next)?;
    let mut line = format!("{}: {}, written to {}", IFACE, PREFERRED, path.display());
    if desktop_file {
        line.push_str(", the file xdg-desktop-portal reads on this desktop");
    }
    match before.filter(|had| !is_fleas(had)) {
        Some(had) => line.push_str(&format!("; it named {} before, and the undo below puts that back", had)),
        None => line.push_str("; every other interface keeps the routing it had"),
    }
    Ok(line)
}

// Every user portal file is cleaned: an older Flea wrote portals.conf even where a desktop file hid it.
fn release_chooser() -> Result<String, String> {
    let plain = conf_path()?;
    let mut lines = Vec::new();
    for desktop in desktop_files()? {
        lines.extend(release_file(&desktop, false)?);
    }
    lines.extend(release_file(&plain, true)?);
    if lines.is_empty() {
        return Ok(format!("{}: nothing to undo, no portal configuration names Flea", IFACE));
    }
    Ok(lines.join("\n"))
}

// One file's undo; `own_file` is portals.conf, which Flea may have created and so may remove.
fn release_file(path: &Path, own_file: bool) -> Result<Option<String>, String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} could not be read: {}", path.display(), io_message(&e))),
    };
    let Some(next) = drop_preferred(&text, IFACE) else {
        return Ok(None);
    };
    // A file left holding nothing but the group heading was this command's own, so it goes with the key.
    if own_file && next.trim() == GROUP {
        fs::remove_file(path).map_err(|e| format!("{} could not be removed: {}", path.display(), io_message(&e)))?;
        return Ok(Some(format!("{}: Flea's line removed, and {} held nothing else, so it is gone", IFACE, path.display())));
    }
    let restored = preferred_value(&next, IFACE);
    replace_file(path, &next)?;
    Ok(Some(match restored {
        Some(had) => format!("{}: {} put back in {}", IFACE, had, path.display()),
        None => format!("{}: Flea's line removed from {}", IFACE, path.display()),
    }))
}

fn conf_path() -> Result<PathBuf, String> {
    Ok(config_home()?.join("xdg-desktop-portal").join("portals.conf"))
}

// The one user file xdg-desktop-portal reads, and whether it is the desktop-specific one.
fn effective_conf(command: &str) -> Result<(PathBuf, bool), String> {
    if let Some(desktop) = shadowing_file()? {
        return Ok((desktop, true));
    }
    // With no desktop named, a desktop file here may be the one the portal reads, so portals.conf would be a guess.
    let found = desktop_files()?;
    if std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().is_empty() && !found.is_empty() {
        return Err(unknown_desktop(&found, command));
    }
    Ok((conf_path()?, false))
}

fn unknown_desktop(found: &[PathBuf], command: &str) -> String {
    let names: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
    format!(
        "The desktop is unknown, so Flea cannot tell whether this desktop reads {}. Run {} inside the desktop session.",
        names.join(" or "), command
    )
}

// Inside one directory xdg-desktop-portal reads <desktop>-portals.conf and stops, so a file for this
// desktop hides the plain one entirely. Sample input: XDG_CURRENT_DESKTOP=Hyprland gives hyprland-portals.conf.
fn shadowing_file() -> Result<Option<PathBuf>, String> {
    let dir = config_home()?.join("xdg-desktop-portal");
    let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    for name in desktops.split(':').filter(|d| !d.is_empty()) {
        let candidate = dir.join(format!("{}{}", name.to_lowercase(), DESKTOP_SUFFIX));
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

// Undo reads the directory rather than the session, so an off run over ssh still finds the file a claim wrote.
// Sample input: ~/.config/xdg-desktop-portal holding portals.conf and hyprland-portals.conf gives the second.
fn desktop_files() -> Result<Vec<PathBuf>, String> {
    let dir = config_home()?.join("xdg-desktop-portal");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{} could not be read: {}", dir.display(), io_message(&e))),
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(DESKTOP_SUFFIX)))
        .collect();
    found.sort();
    Ok(found)
}

// Proof the package landed, the same search defaults::installed_entry() makes for its own file.
// corner: xdg-desktop-portal 1.22.1 reads its own datadir too, so a session narrowing XDG_DATA_DIRS
// off /usr/share is refused here rather than claimed behind a --default that refused the same box.
fn installed_portal() -> Option<PathBuf> {
    data_file(&format!("xdg-desktop-portal/portals/{}", PORTAL_FILE))
}

// portals.conf(5) is a key file, of which only one key in one group is Flea's:
//   [preferred]
//   default=hyprland;gtk
//   # flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk
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
    // A backend that is not Flea's earns a note; an older note survives only over a Flea-spelled line it still explains.
    let had = preferred_value(text, key);
    let note = had.clone().filter(|had| had.as_str() != value && !is_fleas(had));
    let keep_old_note = had.as_deref().is_some_and(is_fleas);
    let mut out = String::with_capacity(text.len() + line.len() * 2);
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
        } else if in_group && !keep_old_note && is_marker(body, key) {
            continue;
        } else if in_group && !written {
            if let Some(had) = key_value(body, key) {
                if had == value {
                    return None;
                }
                if let Some(had) = &note {
                    out.push_str(&format!("{}{}={}\n", REPLACED, key, had));
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

// Flea's `key` line in [preferred] goes, or becomes the backend a REPLACED note names; None when no line is Flea's.
pub fn drop_preferred(text: &str, key: &str) -> Option<String> {
    let replaced = group_lines(text).find_map(|body| body.strip_prefix(REPLACED).and_then(|l| key_value(l, key)).map(str::to_string));
    let mut out = String::with_capacity(text.len());
    let mut in_group = false;
    let mut changed = false;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\n', '\r']);
        if body.trim_start().starts_with('[') {
            in_group = body.trim() == GROUP;
        } else if in_group && is_marker(body, key) {
            continue;
        } else if in_group && key_value(body, key).is_some_and(is_fleas) {
            changed = true;
            if let Some(had) = &replaced {
                out.push_str(&format!("{}={}\n", key, had));
            }
            continue;
        }
        out.push_str(raw);
    }
    changed.then_some(out)
}

// Sample input: "[preferred]\ndefault=gtk\n<key>=gnome;gtk\n" gives Some("gnome;gtk").
fn preferred_value(text: &str, key: &str) -> Option<String> {
    group_lines(text).find_map(|body| key_value(body, key)).map(str::to_string)
}

// Sample input: "[other]\na=b\n[preferred]\nc=d\n" gives only "c=d".
fn group_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut in_group = false;
    text.lines().filter(move |body| {
        if body.trim_start().starts_with('[') {
            in_group = body.trim() == GROUP;
            return false;
        }
        in_group
    })
}

// Sample input: org.freedesktop.impl.portal.FileChooser=gnome;gtk gives Some("gnome;gtk").
fn key_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')).map(str::trim)
}

// Sample input: "# flea replaced: <key>=gnome;gtk" is a marker for <key>.
fn is_marker(body: &str, key: &str) -> bool {
    body.strip_prefix(REPLACED).is_some_and(|rest| key_value(rest, key).is_some())
}

// Flea's own value, and the bare "flea" an earlier hand edit may have used.
fn is_fleas(value: &str) -> bool {
    value.split(';').next() == Some("flea")
}

#[cfg(test)]
#[path = "chooser_tests.rs"]
mod tests;
