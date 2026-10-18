// flea --default: the per-user steps pacman cannot own, see docs/install.md "Make Flea the default".
use crate::error::io_message;
use crate::hyprkeys;
use crate::userfile::{config_home, create_file, data_file, data_home, replace_file};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// The entry packaging/ installs; the desktop resolves the id to that file, so a missing file is a claim on nothing.
pub const DESKTOP_ID: &str = "com.thisisgm.flea.desktop";
// Directories only: the entry registers nothing else, and a file manager that takes image or archive types is a bad citizen.
const MIME: &str = "inode/directory";
// The bus name a desktop's "Show in folder" calls, which nautilus, dolphin, thunar and nemo each register for too.
const BUS_NAME: &str = "org.freedesktop.FileManager1";
// Flea's own packaged registration, read for its Exec so this never invents an install path.
const PACKAGED_SERVICE: &str = "dbus-1/services/com.thisisgm.flea.FileManager1.service";
// The provenance line, and the test for whether a file already there is Flea's to rewrite or remove.
const MARK: &str = "# Written by `flea --default`; `flea --default off` removes it.";

// Exit statuses: 2 is usage and 1 any other refusal; ui/js/MakeDefault.js finished() reads only 3, and 4 keeps an xdg-mime refusal apart from the rest.
pub const REFUSED: i32 = 1;
pub const EXIT_UNINSTALLED: i32 = 3;
pub const EXIT_XDG_MIME: i32 = 4;

// One refused half: the sentence it shows, and the exit status that stands for what kind of refusal it was.
pub struct Refusal {
    text: String,
    status: i32,
}

// A refusal no caller reads by kind is the generic one.
impl From<String> for Refusal {
    fn from(text: String) -> Refusal {
        Refusal { text, status: REFUSED }
    }
}

// flea --default
pub fn claim() -> i32 {
    claim_from(installed_entry())
}

// Split from claim() so a test can answer for a build that has no desktop entry.
fn claim_from(entry: Option<PathBuf>) -> i32 {
    if entry.is_none() {
        eprintln!(
            "flea: {} is not installed in any applications folder, so nothing was made the default. Install the package first.",
            DESKTOP_ID
        );
        return EXIT_UNINSTALLED;
    }
    report(claim_mime(), claim_service(), hyprkeys::claim())
}

// flea --default off
pub fn release() -> i32 {
    report(release_mime(), release_service(), hyprkeys::release())
}

// Each half stands on its own, so a failure in one still leaves the others' lines on screen.
fn report(mime: Result<String, Refusal>, reveal: Result<String, String>, keys: Result<String, String>) -> i32 {
    let mut status = 0;
    for half in [mime, reveal.map_err(Refusal::from), keys.map_err(Refusal::from)] {
        match half {
            Ok(line) => println!("{}", line),
            Err(refusal) => {
                eprintln!("flea: {}", refusal.text);
                // The first refusal names the kind, since the halves run in order and the first is the one the user fixes first.
                if status == 0 {
                    status = refusal.status;
                }
            }
        }
    }
    status
}

fn claim_mime() -> Result<String, Refusal> {
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
        )
        .into());
    }
    Ok(format!(
        "{}: {}, was {}; written to {} by xdg-mime",
        MIME,
        DESKTOP_ID,
        handler_name(&was),
        mimeapps_path()?.display()
    ))
}

fn release_mime() -> Result<String, Refusal> {
    let path = mimeapps_path()?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("{}: nothing to undo, {} does not exist", MIME, path.display()));
        }
        Err(e) => return Err(format!("{} could not be read: {}", path.display(), io_message(&e)).into()),
    };
    let Some(without) = drop_default(&text, MIME, DESKTOP_ID) else {
        return Ok(format!("{}: nothing to undo, {} does not name {}", MIME, path.display(), DESKTOP_ID));
    };
    replace_file(&path, &without)?;
    let now = query_default()?;
    Ok(format!("{}: now {}, Flea's line removed from {}", MIME, handler_name(&now), path.display()))
}

// D-Bus keeps the FIRST registration for a name it reads, and it reads the data home before every
// system directory, so a file here outranks the four packaged rivals without touching any of them.
fn claim_service() -> Result<String, String> {
    let path = service_path()?;
    // No packaged registration is nothing to put in front, the picker step's shape of precondition rather than a failure.
    let Some(packaged) = data_file(PACKAGED_SERVICE) else {
        return Ok(format!("{}: skipped, {} is not installed in any data folder", BUS_NAME, PACKAGED_SERVICE));
    };
    let want = service_text(&packaged_exec(&packaged)?);
    match fs::read_to_string(&path) {
        Ok(held) if held == want => Ok(format!("{}: already Flea's, in {}", BUS_NAME, path.display())),
        Ok(held) if held.starts_with(MARK) => {
            replace_file(&path, &want)?;
            Ok(format!("{}: Flea's, {} rewritten because the packaged registration changed", BUS_NAME, path.display()))
        }
        Ok(_) => Err(format!(
            "{} is already there and Flea did not write it, so it was left alone; remove it yourself to let Flea answer {}",
            path.display(),
            BUS_NAME
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let dir = service_dir()?;
            fs::create_dir_all(&dir).map_err(|e| format!("{} could not be created: {}", dir.display(), io_message(&e)))?;
            create_file(&path, &want)?;
            Ok(format!(
                "{}: Flea, written to {}, which is read before every system folder",
                BUS_NAME,
                path.display()
            ))
        }
        Err(e) => Err(format!("{} could not be read: {}", path.display(), io_message(&e))),
    }
}

fn release_service() -> Result<String, String> {
    let path = service_path()?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("{}: nothing to undo, {} does not exist", BUS_NAME, path.display()));
        }
        Err(e) => return Err(format!("{} could not be read: {}", path.display(), io_message(&e))),
    };
    if !text.starts_with(MARK) {
        return Ok(format!("{}: nothing to undo, Flea did not write {}", BUS_NAME, path.display()));
    }
    fs::remove_file(&path).map_err(|e| format!("{} could not be removed: {}", path.display(), io_message(&e)))?;
    // Only the two directories the claim itself may have created: remove_dir refuses a directory
    // that still holds anything, so another application's service file keeps both of them.
    let dir = service_dir()?;
    let pruned = fs::remove_dir(&dir).is_ok() && dir.parent().is_some_and(|up| fs::remove_dir(up).is_ok());
    if pruned {
        return Ok(format!("{}: Flea's registration and the folders it created removed from {}", BUS_NAME, path.display()));
    }
    Ok(format!("{}: Flea's registration removed from {}", BUS_NAME, path.display()))
}

// Whatever the installed registration names, never a path written down here.
fn packaged_exec(packaged: &Path) -> Result<String, String> {
    let text = fs::read_to_string(packaged).map_err(|e| format!("{} could not be read: {}", packaged.display(), io_message(&e)))?;
    exec_line(&text)
        .map(str::to_string)
        .ok_or_else(|| format!("{} names no Exec, so there is nothing for {} to run", packaged.display(), BUS_NAME))
}

// The packaged registration, of which only the Exec is copied:
//   [D-BUS Service]
//   Name=org.freedesktop.FileManager1
//   Exec=/usr/lib/flea/flea-filemanager1
fn exec_line(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix("Exec="))
        .map(str::trim)
        .filter(|exec| !exec.is_empty())
}

// dbus-broker 37 and dbus-daemon 1.16.2 both take a comment here, measured on this box, and today's
// bug was a user-level service file nobody could trace, so the first line says who wrote it.
fn service_text(exec: &str) -> String {
    format!("{}\n[D-BUS Service]\nName={}\nExec={}\n", MARK, BUS_NAME, exec)
}

fn service_dir() -> Result<PathBuf, String> {
    Ok(data_home()?.join("dbus-1").join("services"))
}

fn service_path() -> Result<PathBuf, String> {
    Ok(service_dir()?.join(format!("{}.service", BUS_NAME)))
}

fn handler_name(id: &str) -> &str {
    if id.is_empty() {
        "nothing"
    } else {
        id
    }
}

fn query_default() -> Result<String, Refusal> {
    xdg_mime(&["query", "default", MIME])
}

// xdg-mime keeps its own stderr, so its complaint reaches the user and nothing here restates it.
fn xdg_mime(args: &[&str]) -> Result<String, Refusal> {
    let mut command = if args.first() == Some(&"query") {
        crate::backend::locale::command("xdg-mime")
    } else {
        Command::new("xdg-mime")
    };
    let out = command
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|_| missing_xdg_mime())?;
    if !out.status.success() {
        return Err(exited(args, out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn missing_xdg_mime() -> Refusal {
    Refusal { text: "xdg-mime is not installed. Install xdg-utils.".to_string(), status: EXIT_XDG_MIME }
}

// A signal leaves no status, which reads as -1.
fn exited(args: &[&str], code: Option<i32>) -> Refusal {
    Refusal { text: format!("xdg-mime {} could not finish (exit status {})", args.join(" "), code.unwrap_or(-1)), status: EXIT_XDG_MIME }
}

fn mimeapps_path() -> Result<PathBuf, String> {
    Ok(config_home()?.join("mimeapps.list"))
}

// Proof the package landed, the same search chooser::installed_portal() makes for its own file.
fn installed_entry() -> Option<PathBuf> {
    data_file(&format!("applications/{}", DESKTOP_ID))
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

    const PACKAGED: &str = "[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec=/usr/lib/flea/flea-filemanager1\n";

    // ui/js/MakeDefault.js finished() reads these statuses, so a sentence can change without the row losing its state.
    #[test]
    fn a_build_with_no_desktop_entry_exits_with_its_own_status() {
        assert_eq!(claim_from(None), EXIT_UNINSTALLED);
        assert!(![0, REFUSED, 2, EXIT_XDG_MIME].contains(&EXIT_UNINSTALLED), "usage is 2 and any other refusal is 1");
    }

    #[test]
    fn an_xdg_mime_that_could_not_run_or_finish_exits_with_its_own_status() {
        let finished = exited(&["default", DESKTOP_ID, MIME], Some(3));
        assert_eq!(finished.text, "xdg-mime default com.thisisgm.flea.desktop inode/directory could not finish (exit status 3)");
        assert_eq!(finished.status, EXIT_XDG_MIME);
        let missing = missing_xdg_mime();
        assert_eq!(missing.text, "xdg-mime is not installed. Install xdg-utils.");
        assert_eq!(missing.status, EXIT_XDG_MIME);
        assert_ne!(EXIT_XDG_MIME, EXIT_UNINSTALLED);
    }

    #[test]
    fn the_first_refusal_picks_the_exit_status_and_a_clean_run_exits_zero() {
        let xdg = || Refusal { text: "xdg-mime".to_string(), status: EXIT_XDG_MIME };
        assert_eq!(report(Err(xdg()), Ok("a".to_string()), Err("keys".to_string())), EXIT_XDG_MIME);
        assert_eq!(report(Ok("a".to_string()), Err("reveal".to_string()), Err("keys".to_string())), REFUSED);
        assert_eq!(report(Ok("a".to_string()), Ok("b".to_string()), Ok("c".to_string())), 0);
    }

    #[test]
    fn a_registration_that_cannot_be_read_says_why_in_words() {
        let dir = crate::backend::testdir::TestDir::new("defaultsunreadable");
        let folder = dir.dir("registration.service");
        assert_eq!(packaged_exec(&folder).unwrap_err(), format!("{} could not be read: item is a folder", folder.display()));
    }

    #[test]
    fn the_exec_is_copied_from_the_packaged_registration_and_never_written_down_here() {
        assert_eq!(exec_line(PACKAGED), Some("/usr/lib/flea/flea-filemanager1"));
        // A registration this cannot read an Exec out of is refused rather than guessed at.
        assert_eq!(exec_line("[D-BUS Service]\nName=org.freedesktop.FileManager1\n"), None);
        assert_eq!(exec_line("[D-BUS Service]\nExec=\n"), None);
        assert_eq!(exec_line(""), None);
    }

    #[test]
    fn the_written_file_leads_with_its_provenance_and_carries_the_packaged_exec() {
        let text = service_text(exec_line(PACKAGED).expect("the packaged Exec"));
        assert_eq!(
            text,
            "# Written by `flea --default`; `flea --default off` removes it.\n[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec=/usr/lib/flea/flea-filemanager1\n"
        );
        // The first line is what tells Flea's file from somebody else's, so release reads it too.
        assert!(text.starts_with(MARK));
        assert!(!PACKAGED.starts_with(MARK));
    }

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
