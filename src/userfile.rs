// The per-user preference files flea --default touches: where they are, and the one way they are rewritten.
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

// How a variable is read: production reads the process environment, a test names its own values.
type Get<'a> = &'a dyn Fn(&str) -> Option<std::ffi::OsString>;

fn process_env(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name)
}

// A set but empty XDG variable means unset, the reading xdg-mime and Omarchy's paths.lua both take.
pub fn env_dir(name: &str) -> Option<PathBuf> {
    env_dir_from(&process_env, name)
}

fn env_dir_from(get: Get, name: &str) -> Option<PathBuf> {
    get(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn home() -> Result<PathBuf, String> {
    home_from(&process_env)
}

fn home_from(get: Get) -> Result<PathBuf, String> {
    env_dir_from(get, "HOME").ok_or_else(|| "HOME is not set, so no per-user preference file can be found".to_string())
}

pub fn config_home() -> Result<PathBuf, String> {
    config_home_from(&process_env)
}

fn config_home_from(get: Get) -> Result<PathBuf, String> {
    match env_dir_from(get, "XDG_CONFIG_HOME") {
        Some(p) => Ok(p),
        None => Ok(home_from(get)?.join(".config")),
    }
}

// The one directory D-Bus, xdg-mime and the desktop all read before any system one, which is what
// makes a file written here outrank a packaged one.
pub fn data_home() -> Result<PathBuf, String> {
    data_home_from(&process_env)
}

fn data_home_from(get: Get) -> Result<PathBuf, String> {
    match env_dir_from(get, "XDG_DATA_HOME") {
        Some(p) => Ok(p),
        None => Ok(home_from(get)?.join(".local/share")),
    }
}

// The base every cache path joins its leaf onto: XDG_CACHE_HOME when set, else HOME/.cache, else /tmp.
pub fn cache_home() -> PathBuf {
    env_dir("XDG_CACHE_HOME")
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

// The XDG lookup both install proofs read: the data home first, then every data dir, whose own
// default already names /usr/share, where pacman puts a package's files.
fn data_dirs_from(get: Get) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(h) = data_home_from(get) {
        dirs.push(h);
    }
    let system = get("XDG_DATA_DIRS").and_then(|v| v.into_string().ok()).filter(|v| !v.is_empty());
    let system = system.unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs
}

// Where an installed package's own file is looked for, and the one ladder both install proofs read:
// flea --default's desktop entry and flea --picker's portal file, which answered differently about
// the same package until they shared this. Sample input: "applications/com.thisisgm.flea.desktop".
pub fn data_file(relative: &str) -> Option<PathBuf> {
    data_file_from(&process_env, relative)
}

fn data_file_from(get: Get, relative: &str) -> Option<PathBuf> {
    data_dirs_from(get).into_iter().map(|d| d.join(relative)).find(|p| p.is_file())
}

// A user's own state or config file, bounded and opened non-blocking, so a fifo or an endless file in its place is an error.
pub fn read_text(path: &Path, limit: u64) -> std::io::Result<String> {
    crate::backend::regfile::read_text(path, limit)
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

    // A getter over named values, so a test reads variables the real environment does not hold.
    fn named(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> {
        let owned: Vec<(String, String)> = pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect();
        move |name| owned.iter().find(|(key, _)| key == name).map(|(_, value)| std::ffi::OsString::from(value))
    }

    #[test]
    fn config_home_reads_a_non_empty_xdg_config_home_and_falls_back_to_home() {
        let set = named(&[("XDG_CONFIG_HOME", "/tmp/flea-test-xdg"), ("HOME", "/flea-test/home")]);
        assert_eq!(config_home_from(&set).expect("set"), PathBuf::from("/tmp/flea-test-xdg"));
        let empty = named(&[("XDG_CONFIG_HOME", ""), ("HOME", "/flea-test/home")]);
        assert_eq!(config_home_from(&empty).expect("fallback"), PathBuf::from("/flea-test/home/.config"));
        assert!(config_home_from(&named(&[])).is_err(), "no HOME and no XDG_CONFIG_HOME names no directory");
    }

    // A directory only one of the two install proofs searched is what put them out of step about one package.
    #[test]
    fn both_install_proofs_read_one_xdg_ladder() {
        let both = named(&[("XDG_DATA_HOME", "/flea-test/data-home"), ("XDG_DATA_DIRS", "/flea-test/only"), ("HOME", "/flea-test/home")]);
        assert_eq!(
            data_dirs_from(&both),
            vec![PathBuf::from("/flea-test/data-home"), PathBuf::from("/flea-test/only")]
        );
        // The first rung is its own function, because the D-Bus claim writes into exactly that one.
        assert_eq!(data_home_from(&both).expect("set"), PathBuf::from("/flea-test/data-home"));
        let no_home = named(&[("XDG_DATA_HOME", ""), ("XDG_DATA_DIRS", "/flea-test/only"), ("HOME", "/flea-test/home")]);
        assert_eq!(data_home_from(&no_home).expect("fallback"), PathBuf::from("/flea-test/home/.local/share"));
        assert_eq!(data_dirs_from(&no_home)[0], PathBuf::from("/flea-test/home/.local/share"));
        // An empty variable is unset, and the default is where pacman puts the package's files.
        let default_dirs = named(&[("XDG_DATA_HOME", "/flea-test/data-home"), ("XDG_DATA_DIRS", "")]);
        assert_eq!(
            data_dirs_from(&default_dirs),
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
        let (absent, dirs) = (d.join("absent").display().to_string(), d.join("share").display().to_string());
        let found = named(&[("XDG_DATA_HOME", &absent), ("XDG_DATA_DIRS", &dirs)]);
        assert_eq!(data_file_from(&found, "applications/flea.test"), Some(share.join("flea.test")));
        assert_eq!(data_file_from(&found, "applications/flea.missing"), None);
    }
}
