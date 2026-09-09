// The per-user preference files flea --default touches: where they are, and the one way they are rewritten.
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

// A set but empty XDG variable means unset, the reading xdg-mime and Omarchy's paths.lua both take.
pub fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn home() -> Result<PathBuf, String> {
    env_dir("HOME").ok_or_else(|| "HOME is not set, so no per-user preference file can be found".to_string())
}

pub fn config_home() -> Result<PathBuf, String> {
    match env_dir("XDG_CONFIG_HOME") {
        Some(p) => Ok(p),
        None => Ok(home()?.join(".config")),
    }
}

// The one directory D-Bus, xdg-mime and the desktop all read before any system one, which is what
// makes a file written here outrank a packaged one.
pub fn data_home() -> Result<PathBuf, String> {
    match env_dir("XDG_DATA_HOME") {
        Some(p) => Ok(p),
        None => Ok(home()?.join(".local/share")),
    }
}

// The XDG lookup both install proofs read: the data home first, then every data dir, whose own
// default already names /usr/share, where pacman puts a package's files. apps.rs resolves desktop
// entry ids over the same ladder, in the same order gio's own resolution walks it.
pub fn data_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(h) = data_home() {
        dirs.push(h);
    }
    let system = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty());
    let system = system.unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs
}

// Where an installed package's own file is looked for, and the one ladder both install proofs read:
// flea --default's desktop entry and flea --picker's portal file, which answered differently about
// the same package until they shared this. Sample input: "applications/com.thisisgm.flea.desktop".
pub fn data_file(relative: &str) -> Option<PathBuf> {
    data_dirs().into_iter().map(|d| d.join(relative)).find(|p| p.is_file())
}

// The write AGENTS.md "Predictable path writes" describes: exclusive temp file at the original's own mode, then a rename.
pub fn replace_file(path: &Path, text: &str) -> Result<(), String> {
    // Through the symlink a dotfiles manager may have put here, so the link survives and its target is what changes.
    let real = fs::canonicalize(path).map_err(|e| format!("{} could not be resolved ({:?})", path.display(), e.kind()))?;
    let mode = fs::metadata(&real)
        .map_err(|e| format!("{} could not be read ({:?})", real.display(), e.kind()))?
        .permissions()
        .mode()
        & 0o7777;
    let tmp = PathBuf::from(format!("{}.{}.tmp", real.display(), std::process::id()));
    let _ = fs::remove_file(&tmp);
    let written = write_new(&tmp, mode, text).and_then(|()| {
        fs::rename(&tmp, &real).map_err(|e| format!("{} could not replace {} ({:?})", tmp.display(), real.display(), e.kind()))
    });
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

// A file this process is creating for the first time: O_EXCL, so a symlink planted at the path is
// refused rather than followed, and an existing file is a failure and not a silent overwrite.
pub fn create_file(path: &Path, text: &str) -> Result<(), String> {
    write_new(path, 0o644, text)
}

fn write_new(tmp: &Path, mode: u32, text: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(tmp)
        .map_err(|e| format!("{} could not be created ({:?})", tmp.display(), e.kind()))?;
    file.write_all(text.as_bytes())
        .map_err(|e| format!("{} could not be written ({:?})", tmp.display(), e.kind()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn replace_file_writes_through_a_symlink_and_keeps_the_mode() {
        let d = TestDir::new("replace");
        let real = d.file("real.conf", "old\n");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).expect("mode");
        let link = d.join("link.conf");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        replace_file(&link, "new\n").expect("replace");
        assert!(fs::symlink_metadata(&link).expect("link").file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).expect("real"), "new\n");
        assert_eq!(fs::metadata(&real).expect("real").permissions().mode() & 0o777, 0o600);
        // ls -A: the sandbox holds its marker, the file and the link, and no temp file.
        assert_eq!(fs::read_dir(d.path()).expect("dir").count(), 3);
    }

    #[test]
    fn replace_file_leaves_the_original_alone_when_it_cannot_write() {
        let d = TestDir::new("replace-missing");
        assert!(replace_file(&d.join("absent.conf"), "x").is_err());
        assert_eq!(fs::read_dir(d.path()).expect("dir").count(), 1);
    }

    // One test, because the variable is process wide and cargo runs tests in threads.
    #[test]
    fn config_home_reads_a_non_empty_xdg_config_home_and_falls_back_to_home() {
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/flea-test-xdg");
        assert_eq!(config_home().expect("set"), PathBuf::from("/tmp/flea-test-xdg"));
        std::env::set_var("XDG_CONFIG_HOME", "");
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(config_home().expect("fallback"), PathBuf::from(home).join(".config"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    // One test, because the variables are process wide and cargo runs tests in threads. A directory
    // only one of the two install proofs searched is what put them out of step about one package.
    #[test]
    fn both_install_proofs_read_one_xdg_ladder() {
        std::env::set_var("XDG_DATA_HOME", "/flea-test/data-home");
        std::env::set_var("XDG_DATA_DIRS", "/flea-test/only");
        assert_eq!(
            data_dirs(),
            vec![PathBuf::from("/flea-test/data-home"), PathBuf::from("/flea-test/only")]
        );
        // The first rung is its own function, because the D-Bus claim writes into exactly that one.
        assert_eq!(data_home().expect("set"), PathBuf::from("/flea-test/data-home"));
        std::env::set_var("XDG_DATA_HOME", "");
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(data_home().expect("fallback"), PathBuf::from(&home).join(".local/share"));
        assert_eq!(data_dirs()[0], PathBuf::from(&home).join(".local/share"));
        std::env::set_var("XDG_DATA_HOME", "/flea-test/data-home");
        // An empty variable is unset, and the default is where pacman puts the package's files.
        std::env::set_var("XDG_DATA_DIRS", "");
        assert_eq!(
            data_dirs(),
            vec![
                PathBuf::from("/flea-test/data-home"),
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        );

        // The ladder is the half that diverged; this is the join and the find on top of it.
        let d = TestDir::new("data-file");
        let share = d.dir("share/applications");
        fs::write(share.join("flea.test"), "").expect("entry");
        std::env::set_var("XDG_DATA_HOME", d.join("absent").display().to_string());
        std::env::set_var("XDG_DATA_DIRS", d.join("share").display().to_string());
        assert_eq!(data_file("applications/flea.test"), Some(share.join("flea.test")));
        assert_eq!(data_file("applications/flea.missing"), None);

        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("XDG_DATA_DIRS");
    }
}
