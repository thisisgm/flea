// flea --default: the one per-user step pacman cannot own, see docs/install.md "Make Flea the default".
use crate::hyprkeys;
use crate::userfile::{config_home, env_dir, home, replace_file};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// The entry packaging/ installs; the desktop resolves the id to that file, so a missing file is a claim on nothing.
pub const DESKTOP_ID: &str = "com.thisisgm.flea.desktop";
// Directories only: the entry registers nothing else, and a file manager that takes image or archive types is a bad citizen.
const MIME: &str = "inode/directory";

// flea --default
pub fn claim() -> i32 {
    if installed_entry().is_none() {
        eprintln!(
            "flea: {} is not installed in any applications directory, so there is nothing to make the default; install the package first",
            DESKTOP_ID
        );
        return 1;
    }
    report(claim_mime(), hyprkeys::claim())
}

// flea --default off
pub fn release() -> i32 {
    report(release_mime(), hyprkeys::release())
}

// Each half stands on its own, so a failure in one still leaves the other's line on screen.
fn report(mime: Result<String, String>, keys: Result<String, String>) -> i32 {
    let mut status = 0;
    for half in [mime, keys] {
        match half {
            Ok(line) => println!("{}", line),
            Err(why) => {
                eprintln!("flea: {}", why);
                status = 1;
            }
        }
    }
    status
}

fn claim_mime() -> Result<String, String> {
    let was = query_default()?;
    if was == DESKTOP_ID {
        return Ok(format!("{}: already {}", MIME, DESKTOP_ID));
    }
    xdg_mime(&["default", DESKTOP_ID, MIME])?;
    // xdg-mime exits 0 whatever it wrote, so the answer is read back rather than trusted.
    let now = query_default()?;
    if now != DESKTOP_ID {
        return Err(format!(
            "xdg-mime default exited 0 but {} still resolves to {}; the desktop skips an entry whose Exec is not on PATH, so check that flea is",
            MIME,
            handler_name(&now)
        ));
    }
    Ok(format!(
        "{}: {}, was {}; written to {} by xdg-mime",
        MIME,
        DESKTOP_ID,
        handler_name(&was),
        mimeapps_path()?.display()
    ))
}

fn release_mime() -> Result<String, String> {
    let path = mimeapps_path()?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("{}: nothing to undo, {} does not exist", MIME, path.display()));
        }
        Err(e) => return Err(format!("{} could not be read ({:?})", path.display(), e.kind())),
    };
    let Some(without) = drop_default(&text, MIME, DESKTOP_ID) else {
        return Ok(format!("{}: nothing to undo, {} does not name {}", MIME, path.display(), DESKTOP_ID));
    };
    replace_file(&path, &without)?;
    let now = query_default()?;
    Ok(format!("{}: now {}, Flea's line removed from {}", MIME, handler_name(&now), path.display()))
}

fn handler_name(id: &str) -> &str {
    if id.is_empty() {
        "nothing"
    } else {
        id
    }
}

fn query_default() -> Result<String, String> {
    xdg_mime(&["query", "default", MIME])
}

// xdg-mime keeps its own stderr, so its complaint reaches the user and nothing here restates it.
fn xdg_mime(args: &[&str]) -> Result<String, String> {
    let out = Command::new("xdg-mime")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|_| "xdg-mime is not on PATH; it ships in xdg-utils, which the package depends on".to_string())?;
    if !out.status.success() {
        return Err(format!("xdg-mime {} exited {}", args.join(" "), out.status.code().unwrap_or(-1)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn mimeapps_path() -> Result<PathBuf, String> {
    Ok(config_home()?.join("mimeapps.list"))
}

// The lookup xdg-mime makes when it resolves an id: the data home first, then every data dir.
fn installed_entry() -> Option<PathBuf> {
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
    dirs.into_iter().map(|d| d.join("applications").join(DESKTOP_ID)).find(|p| p.is_file())
}

// The per-user file xdg-mime writes, of which only the [Default Applications] section is ours to touch:
//   [Default Applications]
//   inode/directory=com.thisisgm.flea.desktop
//   image/png=imv.desktop
// Returns the file without Flea's claim on `mime`, or None when the file makes no such claim.
pub fn drop_default(text: &str, mime: &str, id: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut in_defaults = false;
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let newline = &line[body.len()..];
        if body.starts_with('[') {
            in_defaults = body == "[Default Applications]";
        } else if in_defaults {
            if let Some(value) = body.strip_prefix(mime).and_then(|rest| rest.strip_prefix('=')) {
                // A value is a semicolon list: gio writes a trailing semicolon and xdg-mime writes none.
                let all: Vec<&str> = value.split(';').filter(|v| !v.is_empty()).collect();
                let kept: Vec<&str> = all.iter().copied().filter(|v| *v != id).collect();
                if kept.len() != all.len() {
                    changed = true;
                    if !kept.is_empty() {
                        out.push_str(mime);
                        out.push('=');
                        out.push_str(&kept.join(";"));
                        out.push_str(newline);
                    }
                    continue;
                }
            }
        }
        out.push_str(line);
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

    const OMARCHY_SHAPE: &str = "[Default Applications]\ninode/directory=com.thisisgm.flea.desktop\nimage/png=imv.desktop\n\n[Added Associations]\ninode/directory=com.thisisgm.flea.desktop;\n";

    #[test]
    fn drop_default_removes_only_fleas_line_in_the_default_section() {
        let out = drop_default(OMARCHY_SHAPE, MIME, DESKTOP_ID).expect("the file names Flea");
        assert_eq!(out, "[Default Applications]\nimage/png=imv.desktop\n\n[Added Associations]\ninode/directory=com.thisisgm.flea.desktop;\n");
    }

    #[test]
    fn drop_default_leaves_a_file_that_does_not_name_flea_alone() {
        assert_eq!(drop_default("[Default Applications]\ninode/directory=thunar.desktop\n", MIME, DESKTOP_ID), None);
        assert_eq!(drop_default("inode/directory=com.thisisgm.flea.desktop\n", MIME, DESKTOP_ID), None);
        assert_eq!(drop_default("", MIME, DESKTOP_ID), None);
    }

    #[test]
    fn drop_default_keeps_the_rest_of_a_list_value() {
        // gio writes a trailing semicolon where xdg-mime writes none; both are one claim.
        assert_eq!(drop_default("[Default Applications]\ninode/directory=com.thisisgm.flea.desktop;\n", MIME, DESKTOP_ID), Some("[Default Applications]\n".to_string()));
        assert_eq!(
            drop_default("[Default Applications]\ninode/directory=com.thisisgm.flea.desktop;thunar.desktop;\n", MIME, DESKTOP_ID),
            Some("[Default Applications]\ninode/directory=thunar.desktop\n".to_string())
        );
    }

    #[test]
    fn drop_default_keeps_a_last_line_with_no_newline_intact() {
        let out = drop_default("[Default Applications]\ninode/directory=com.thisisgm.flea.desktop\nimage/png=imv.desktop", MIME, DESKTOP_ID);
        assert_eq!(out, Some("[Default Applications]\nimage/png=imv.desktop".to_string()));
    }
}
