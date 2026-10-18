use super::Theme;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ThemePaths {
    pub current: PathBuf,
    pub user_shell: PathBuf,
    pub fontconfig: PathBuf,
    pub hypr_config: PathBuf,
    pub toggles: PathBuf,
}

impl ThemePaths {
    // Commons uses HOME for theme/font paths; only Glass's toggle directory follows XDG_STATE_HOME.
    pub fn for_home(home: &Path, state_home: Option<&Path>) -> Self {
        Self {
            current: home.join(".local/state/omarchy/current"),
            user_shell: home.join(".config/omarchy/shell.toml"),
            fontconfig: home.join(".config/fontconfig/fonts.conf"),
            hypr_config: home.join(".config/hypr"),
            toggles: state_home
                .map(Path::to_owned)
                .unwrap_or_else(|| home.join(".local/state"))
                .join("omarchy/toggles/hypr"),
        }
    }

    pub fn watched(&self) -> Vec<PathBuf> {
        vec![
            self.current.clone(),
            self.current.join("theme"),
            self.current.join("theme.name"),
            self.current.join("theme/colors.toml"),
            self.current.join("theme/shell.toml"),
            self.user_shell.clone(),
            self.fontconfig.clone(),
            self.hypr_config.clone(),
            self.hypr_config.join("hyprland.conf"),
            self.hypr_config.join("hyprland.lua"),
            self.current.join("theme/hyprland.lua"),
            self.toggles.clone(),
            self.toggles.join("window-opaque.lua"),
            self.toggles.join("window-glassy.lua"),
            self.toggles.join("window-no-gaps.lua"),
        ]
    }

    // Run on a background thread; missing optional shell files mean Commons defaults, other errors propagate.
    pub fn load(&self, stop: Option<u32>, resolved_family: &str) -> io::Result<Theme> {
        let colors = fs::read_to_string(self.current.join("theme/colors.toml"))?;
        let shell = optional(&self.current.join("theme/shell.toml"))?;
        let user = optional(&self.user_shell)?;
        Ok(Theme::resolve(
            &colors,
            &shell,
            &user,
            stop,
            resolved_family,
        ))
    }
}

fn optional(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub path: PathBuf,
    pub target: Option<PathBuf>,
    pub modified: Option<(i64, i64)>,
    pub changed: Option<(i64, i64)>,
    pub identity: Option<(u64, u64, u64)>,
    pub error: Option<io::ErrorKind>,
}

impl FileStamp {
    pub fn read(path: PathBuf) -> Self {
        let mut out = Self {
            path,
            target: None,
            modified: None,
            changed: None,
            identity: None,
            error: None,
        };
        match fs::metadata(&out.path) {
            Ok(m) => {
                out.modified = Some((m.mtime(), m.mtime_nsec()));
                out.changed = Some((m.ctime(), m.ctime_nsec()));
                out.identity = Some((m.dev(), m.ino(), m.len()));
            }
            Err(e) => out.error = Some(e.kind()),
        }
        out.target = fs::read_link(&out.path).ok();
        out
    }
}

#[derive(Clone, Debug)]
pub struct ThemeWatcher {
    paths: ThemePaths,
    stamps: Vec<FileStamp>,
}

impl ThemeWatcher {
    pub fn new(paths: ThemePaths) -> Self {
        let stamps = paths.watched().into_iter().map(FileStamp::read).collect();
        Self { paths, stamps }
    }

    pub fn stamps(&self) -> &[FileStamp] {
        &self.stamps
    }

    // Poll off the UI thread; fresh pathname lookups survive directory replacement and symlink retargeting.
    pub fn poll(&mut self) -> bool {
        let next: Vec<_> = self
            .paths
            .watched()
            .into_iter()
            .map(FileStamp::read)
            .collect();
        let changed = next != self.stamps;
        self.stamps = next;
        changed
    }
}
