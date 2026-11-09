//! Desktop application icon lookup, called only on a worker before an icon reaches the painter.
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

const MAX_VISITED_THEMES: usize = 64;

#[derive(Clone, Debug)]
pub struct Icons {
    pub theme: String,
    pub bases: Vec<PathBuf>,
    pub pixmaps: Vec<PathBuf>,
}

impl Icons {
    pub fn session() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| Path::new(s).is_absolute())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|p| p.join(".config")));
        let theme = std::env::var("QS_ICON_THEME")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                home.as_ref().and_then(|p| {
                    std::fs::read_to_string(
                        p.join(".local/state/omarchy/current/theme/icons.theme"),
                    )
                    .ok()
                })
            })
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                config
                    .and_then(|p| std::fs::read_to_string(p.join("gtk-3.0/settings.ini")).ok())
                    .and_then(|text| {
                        key(&ini(&text), "Settings", "gtk-icon-theme-name").map(str::to_owned)
                    })
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "hicolor".into())
            .trim()
            .to_string();
        Self::from_data(
            &theme,
            home.as_deref(),
            std::env::var_os("XDG_DATA_HOME").as_deref(),
            std::env::var_os("XDG_DATA_DIRS").as_deref(),
        )
    }

    pub fn from_data(
        theme: &str,
        home: Option<&Path>,
        data_home: Option<&std::ffi::OsStr>,
        data_dirs: Option<&std::ffi::OsStr>,
    ) -> Self {
        let mut data = Vec::new();
        if let Some(path) = data_home.filter(|s| Path::new(s).is_absolute()) {
            data.push(PathBuf::from(path));
        } else if let Some(home) = home {
            data.push(home.join(".local/share"));
        }
        let defaults = OsString::from("/usr/local/share:/usr/share");
        data.extend(std::env::split_paths(
            data_dirs.filter(|s| !s.is_empty()).unwrap_or(&defaults),
        ));
        data.retain(|p| p.is_absolute());
        let mut bases: Vec<PathBuf> = home
            .filter(|p| p.is_absolute())
            .map(|p| p.join(".icons"))
            .into_iter()
            .collect();
        bases.extend(data.iter().map(|p| p.join("icons")));
        let mut pixmaps: Vec<_> = data.iter().map(|p| p.join("pixmaps")).collect();
        let system_pixmaps = PathBuf::from("/usr/share/pixmaps");
        if !pixmaps.contains(&system_pixmaps) {
            pixmaps.push(system_pixmaps);
        }
        Self {
            theme: theme.into(),
            bases,
            pixmaps,
        }
    }

    /// Missing icons leave the caller's muted app-window glyph standing, as MenuRow.qml does.
    pub fn resolve(&self, name: &str, size: u32, scale: u32) -> Option<PathBuf> {
        let decoded = crate::ui_logic_context_menus::places::decode_file_uri(name);
        let direct = Path::new(decoded.as_deref().unwrap_or(name));
        if direct.is_absolute() {
            return direct.is_file().then(|| direct.to_path_buf());
        }
        if !component(name) || size == 0 || scale == 0 {
            return None;
        }
        let mut seen = HashSet::new();
        self.inherit(name, size, scale, &self.theme, &mut seen)
            .or_else(|| self.inherit(name, size, scale, "hicolor", &mut seen))
            .or_else(|| {
                self.bases
                    .iter()
                    .chain(&self.pixmaps)
                    .find_map(|p| icon(p, name))
            })
    }

    fn inherit(
        &self,
        name: &str,
        size: u32,
        scale: u32,
        theme: &str,
        seen: &mut HashSet<String>,
    ) -> Option<PathBuf> {
        if !component(theme) || seen.len() >= MAX_VISITED_THEMES || !seen.insert(theme.into()) {
            return None;
        }
        let entries = self
            .bases
            .iter()
            .find_map(|p| std::fs::read_to_string(p.join(theme).join("index.theme")).ok())?;
        let entries = ini(&entries);
        let dirs = ["Directories", "ScaledDirectories"]
            .into_iter()
            .filter_map(|k| key(&entries, "Icon Theme", k))
            .flat_map(|s| s.split(','))
            .map(str::trim);
        let mut nearest: Option<(u64, PathBuf)> = None;
        for dir in dirs {
            if !relative(dir) {
                continue;
            }
            let Some(nominal) = number(&entries, dir, "Size") else {
                continue;
            };
            let factor = number(&entries, dir, "Scale").unwrap_or(1).max(1);
            let (min, max) = match key(&entries, dir, "Type").unwrap_or("Threshold") {
                "Fixed" => (nominal, nominal),
                "Scalable" => (
                    number(&entries, dir, "MinSize").unwrap_or(nominal),
                    number(&entries, dir, "MaxSize").unwrap_or(nominal),
                ),
                _ => {
                    let threshold = number(&entries, dir, "Threshold").unwrap_or(2);
                    (
                        nominal.saturating_sub(threshold),
                        nominal.saturating_add(threshold),
                    )
                }
            };
            let pixels = u64::from(size) * u64::from(scale);
            let low = u64::from(min) * u64::from(factor);
            let high = u64::from(max) * u64::from(factor);
            let distance = if pixels < low {
                low - pixels
            } else {
                pixels.saturating_sub(high)
            };
            for base in &self.bases {
                if let Some(path) = icon(&base.join(theme).join(dir), name) {
                    if factor == scale && (min..=max).contains(&size) {
                        return Some(path);
                    }
                    if nearest.as_ref().map_or(true, |(best, _)| distance < *best) {
                        nearest = Some((distance, path));
                    }
                }
            }
        }
        if let Some((_, path)) = nearest {
            return Some(path);
        }
        key(&entries, "Icon Theme", "Inherits")
            .into_iter()
            .flat_map(|s| s.split(','))
            .map(str::trim)
            .find_map(|parent| self.inherit(name, size, scale, parent, seen))
    }
}

type Ini = HashMap<String, HashMap<String, String>>;
// Example input: "[Icon Theme]\nDirectories=18/apps\n[18/apps]\nSize=18\nType=Fixed\n".
fn ini(text: &str) -> Ini {
    let mut entries = Ini::new();
    let mut section = String::new();
    for line in text.lines().map(str::trim).filter(|s| !s.starts_with('#')) {
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name.into();
        } else if let Some((key, value)) = line.split_once('=') {
            entries
                .entry(section.clone())
                .or_default()
                .entry(key.trim().into())
                .or_insert_with(|| value.trim().into());
        }
    }
    entries
}
fn key<'a>(entries: &'a Ini, section: &str, name: &str) -> Option<&'a str> {
    entries.get(section)?.get(name).map(String::as_str)
}
fn number(entries: &Ini, section: &str, name: &str) -> Option<u32> {
    key(entries, section, name)?.parse().ok()
}
fn component(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\0') && name != "." && name != ".."
}
fn relative(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
fn icon(dir: &Path, name: &str) -> Option<PathBuf> {
    if matches!(
        Path::new(name).extension().and_then(|s| s.to_str()),
        Some("png" | "svg" | "xpm")
    ) {
        let path = dir.join(name);
        return path.is_file().then_some(path);
    }
    ["png", "svg", "xpm"]
        .iter()
        .map(|ext| dir.join(format!("{name}.{ext}")))
        .find(|p| p.is_file())
}

#[cfg(test)]
#[path = "m9_g2rows_03_icons_tests.rs"]
mod tests;
