// Explicit mount/socket mappings; no rclone configuration or credential discovery.
use crate::jsondoc::{self, Json};
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

extern "C" { fn geteuid() -> u32; }

pub(super) fn socket_for(mount: &Path) -> Result<Option<PathBuf>, &'static str> {
    let base = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("Configuration directory is unavailable")?;
    if !base.is_absolute() { return Err("Configuration directory must be absolute"); }
    let file = match crate::backend::regfile::open_if_regular(&base.join("flea/rclone-status.json"), crate::oflags::O_NOFOLLOW) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Upload monitoring configuration is unreadable"),
    };
    let meta = file.metadata().map_err(|_| "Cannot inspect upload configuration")?;
    if !meta.is_file() || meta.uid() != unsafe { geteuid() } || meta.mode() & 0o022 != 0 {
        return Err("Upload configuration must be a user-owned, non-writable-by-others file");
    }
    let mut body = String::new();
    file.take(65_537).read_to_string(&mut body).map_err(|_| "Cannot read upload configuration")?;
    if body.len() > 65_536 { return Err("Upload configuration is too large"); }
    mapping(&body, mount)
}

fn mapping(body: &str, mount: &Path) -> Result<Option<PathBuf>, &'static str> {
    let json = jsondoc::parse(body).map_err(|_| "Invalid upload monitoring JSON")?;
    let entries = json.get("mounts").and_then(Json::as_array).ok_or("Expected a mounts array")?;
    if entries.len() > 32 { return Err("Too many upload monitoring mappings"); }
    let mut found = None;
    for entry in entries {
        let path = entry.get("mount").and_then(Json::as_str).ok_or("Mapping is missing mount")?;
        let socket = entry.get("socket").and_then(Json::as_str).ok_or("Mapping is missing socket")?;
        if !Path::new(path).is_absolute() || !Path::new(socket).is_absolute() {
            return Err("Mount and socket paths must be absolute");
        }
        if Path::new(path) == mount {
            if found.is_some() { return Err("Duplicate upload monitoring mapping"); }
            found = Some(PathBuf::from(socket));
        }
    }
    Ok(found)
}

pub(super) fn private_socket(socket: &Path) -> Result<(), &'static str> {
    let parent = socket.parent().ok_or("Socket has no parent directory")?;
    let uid = unsafe { geteuid() };
    let directory = parent.metadata().map_err(|_| "Status socket directory is unavailable")?;
    let meta = socket.symlink_metadata().map_err(|_| "Status socket is unavailable")?;
    if !directory.is_dir() || directory.uid() != uid || directory.mode() & 0o077 != 0
        || !meta.file_type().is_socket() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err("Status socket and its directory must be private to the current user");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mappings_are_exact_and_ambiguous_or_network_endpoints_are_refused() {
        let body = r#"{"mounts":[{"mount":"/Cloud","socket":"/run/private/rc.sock"}]}"#;
        assert_eq!(mapping(body, Path::new("/Cloud")).unwrap(), Some("/run/private/rc.sock".into()));
        assert!(mapping(body, Path::new("/Cloud-other")).unwrap().is_none());
        assert!(mapping(r#"{"mounts":[{"mount":"/Cloud","socket":"http://localhost:5572"}]}"#, Path::new("/Cloud")).is_err());
        assert!(mapping(r#"{"mounts":[{"mount":"/Cloud","socket":"/a"},{"mount":"/Cloud","socket":"/b"}]}"#, Path::new("/Cloud")).is_err());
    }
    #[test]
    fn shared_sockets_and_symlinks_are_not_contacted() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let d = crate::backend::testdir::TestDir::new("cloud-socket");
        let private = d.dir("private");
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = private.join("rc.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(private_socket(&socket).is_ok());
        let link = private.join("link"); symlink(&socket, &link).unwrap();
        assert!(private_socket(&link).is_err());
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_socket(&socket).is_err());
    }
}
