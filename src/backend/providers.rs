// Menu capability discovery reads metadata only; provider status runs on explicit UI entry.
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn command(name: &str, paths: &[PathBuf]) -> (bool, String, String) {
    const ENOTDIR: i32 = 20;
    let mut unavailable = None;
    for directory in paths {
        let path = directory.join(name);
        match path.metadata() {
            Ok(meta) if meta.is_file() && meta.permissions().mode() & 0o111 != 0 => {
                return (true, path.exact_text().into_owned(), String::new())
            }
            Ok(_) => unavailable = Some(format!("{} is not executable.", path.display())),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(ENOTDIR) =>
            {
                if path.symlink_metadata().is_ok() {
                    unavailable = Some(format!(
                        "{} is a link to an item that is gone.",
                        path.display()
                    ));
                }
            }
            Err(error) => {
                unavailable = Some(format!(
                    "Could not inspect {}: {}.",
                    path.display(),
                    crate::error::io_message(&error)
                ))
            }
        }
    }
    match unavailable {
        Some(reason) => (true, String::new(), reason),
        None => (false, String::new(), format!("{} is not installed.", name)),
    }
}

fn command_json(name: &str, paths: &[PathBuf]) -> String {
    let (installed, command, reason) = command(name, paths);
    format!(r#"{{"installed":{},"command":"{}","reason":"{}"}}"#, installed, escape(&command), escape(&reason))
}

// Directive 71: what Flea can drive is localsend-cli, not the app's own window, so the row is offered
// when that is on PATH. The helper still takes a list, because a rename upstream is one name away.
fn command_json_any(names: &[&str], paths: &[PathBuf]) -> String {
    let mut answer = (false, String::new(), String::new());
    for name in names {
        answer = command(name, paths);
        if answer.0 && !answer.1.is_empty() { break; }
    }
    if !answer.0 { answer.2 = format!("{} is not installed.", names.join(" or ")); }
    format!(r#"{{"installed":{},"command":"{}","reason":"{}"}}"#, answer.0, escape(&answer.1), escape(&answer.2))
}

fn dropbox_info(path: &Path) -> Result<String, String> {
    // Account metadata is small; refuse oversized or non-regular input without reading a tree or blocking on a FIFO.
    const MAX_ACCOUNT_BYTES: u64 = 64 * 1024;
    let file = match super::regfile::open_if_regular(path, 0) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(format!("Could not read Dropbox account: {}.", crate::error::io_message(&error))),
    };
    let mut text = String::new();
    file.take(MAX_ACCOUNT_BYTES + 1).read_to_string(&mut text)
        .map_err(|error| format!("Could not read Dropbox account: {}.", crate::error::io_message(&error)))?;
    if text.len() as u64 > MAX_ACCOUNT_BYTES { return Err("Dropbox account metadata is too large.".into()); }
    Ok(text)
}

// Sample input: Some("/home/gm") reads /home/gm/.dropbox/info.json; None, with no home to look in, is an error.
fn account_info(home: Option<std::ffi::OsString>) -> Result<String, String> {
    home.ok_or_else(|| "The Dropbox account folder is unavailable.".to_string())
        .and_then(|home| dropbox_info(&PathBuf::from(home).join(".dropbox/info.json")))
}

pub(crate) fn facts() -> String {
    let paths: Vec<PathBuf> = std::env::var_os("PATH").map(|value| std::env::split_paths(&value).collect()).unwrap_or_default();
    let info = account_info(std::env::var_os("HOME"));
    let (text, error) = match info { Ok(text) => (text, String::new()), Err(error) => (String::new(), error) };
    format!(r#"{{"taildrop":{},"taildropSend":{},"localsend":{},"rclone":{},"dropbox":{},"owly":{},"dropboxInfo":"{}","dropboxError":"{}"}}"#,
        command_json("tailscale", &paths), command_json("omarchy-tailscale-send", &paths),
        command_json_any(&["localsend-cli"], &paths),
        command_json("rclone", &paths),
        command_json("dropbox-cli", &paths), command_json("owly", &paths), escape(&text), escape(&error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn absence_and_an_unusable_installation_remain_distinct() {
        let root = TestDir::new("providers");
        let paths = vec![root.path().to_path_buf()];
        assert!(!command("tailscale", &paths).0);
        let binary = root.file("tailscale", "exit 0\n");
        let result = command("tailscale", &paths);
        assert!(result.0 && result.1.is_empty() && result.2.contains("not executable"));
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(command("tailscale", &paths), (true, binary.to_string_lossy().into_owned(), String::new()));
        assert_eq!(dropbox_info(&root.join("missing.json")).unwrap(), "");
        assert!(dropbox_info(&root.dir("directory")).unwrap_err().contains("Could not read"));
        let info = root.file("info.json", r#"{"personal":{"path":"/example/Dropbox"}}"#);
        assert!(dropbox_info(&info).unwrap().contains("personal"));
        std::fs::write(&info, vec![b' '; 64 * 1024 + 1]).unwrap();
        assert!(dropbox_info(&info).unwrap_err().contains("too large"));
    }

    #[test]
    fn a_link_to_an_item_that_is_gone_says_so_and_a_missing_home_names_the_account_folder() {
        let root = TestDir::new("providers-dangling");
        let paths = vec![root.path().to_path_buf()];
        std::os::unix::fs::symlink(root.join("gone"), root.join("tailscale")).unwrap();
        assert_eq!(command("tailscale", &paths).2, format!("{} is a link to an item that is gone.", root.join("tailscale").display()));
        assert_eq!(account_info(None).unwrap_err(), "The Dropbox account folder is unavailable.");
    }

    // Directive 71: the row is offered when localsend-cli is on PATH, which is what the backend
    // drives; the app's own window is never opened, so its wrapper is not what this looks for.
    #[test]
    fn the_row_waits_for_the_cli_the_backend_can_actually_drive() {
        let root = TestDir::new("localsend");
        let paths = vec![root.path().to_path_buf()];
        let absent = command_json_any(&["localsend-cli"], &paths);
        assert!(absent.contains(r#""installed":false"#), "{}", absent);
        assert!(absent.contains("localsend-cli is not installed."), "{}", absent);
        // The app's own wrapper is not the CLI, and a box with only that one offers no row.
        let app = root.file("localsend", "exit 0\n");
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(command_json_any(&["localsend-cli"], &paths).contains(r#""installed":false"#));
        let cli = root.file("localsend-cli", "exit 0\n");
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let found = command_json_any(&["localsend-cli"], &paths);
        assert!(found.contains(r#""installed":true"#), "{}", found);
        assert!(found.contains(&escape(&cli.to_string_lossy())), "{}", found);
    }
}
