// Menu capability discovery reads metadata only; provider status runs on explicit UI entry.
use crate::json::escape;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn command(name: &str, paths: &[PathBuf]) -> (bool, String, String) {
    const ENOTDIR: i32 = 20;
    let mut unavailable = None;
    for directory in paths {
        let path = directory.join(name);
        match path.metadata() {
            Ok(meta) if meta.is_file() && meta.permissions().mode() & 0o111 != 0 =>
                return (true, path.to_string_lossy().into_owned(), String::new()),
            Ok(_) => unavailable = Some(format!("{} is not executable.", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(ENOTDIR) => {
                if path.symlink_metadata().is_ok() {
                    unavailable = Some(format!("{} has a missing target.", path.display()));
                }
            }
            Err(error) => unavailable = Some(format!("Could not inspect {}: {}.", path.display(), crate::error::io_message(&error))),
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

pub(crate) fn facts() -> String {
    let paths: Vec<PathBuf> = std::env::var_os("PATH").map(|value| std::env::split_paths(&value).collect()).unwrap_or_default();
    let info = std::env::var_os("HOME").ok_or_else(|| "Dropbox account home is unavailable.".into())
        .and_then(|home| dropbox_info(&PathBuf::from(home).join(".dropbox/info.json")));
    let (text, error) = match info { Ok(text) => (text, String::new()), Err(error) => (String::new(), error) };
    format!(r#"{{"taildrop":{},"taildropSend":{},"dropbox":{},"dropboxInfo":"{}","dropboxError":"{}"}}"#,
        command_json("tailscale", &paths), command_json("omarchy-tailscale-send", &paths),
        command_json("dropbox-cli", &paths), escape(&text), escape(&error))
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
}
