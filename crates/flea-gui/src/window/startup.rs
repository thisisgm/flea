//! Startup policy shared by window creation and later tab adapters.
use flea::jsondoc::Json;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const LISTING_WAIT_MS: u64 = 10_000;
pub const INSPECT_WAIT_MS: u64 = 10_000;
// user-dirs.dirs is about a dozen lines, so a larger file is not one xdg-user-dirs wrote.
const MAX_USER_DIRS_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arguments {
    pub path: Option<String>,
    pub select: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentError {
    pub key: &'static str,
    pub argument: String,
}

impl Arguments {
    pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, ArgumentError> {
        let error = |key, argument| ArgumentError { key, argument };
        let mut out = Self::default();
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--select" => {
                    out.select = Some(
                        args.next()
                            .ok_or_else(|| error("window.select_required", String::new()))?,
                    )
                }
                "--" => {
                    if let Some(path) = args.next() {
                        if out.path.replace(path).is_some() || args.next().is_some() {
                            return Err(error("window.one_directory", String::new()));
                        }
                    }
                    break;
                }
                flag if flag.starts_with('-') => {
                    return Err(error("window.unknown_flag", argument))
                }
                _ if out.path.is_none() => out.path = Some(argument),
                _ => return Err(error("window.one_directory", String::new())),
            }
        }
        Ok(out)
    }

    pub fn absolutize(&mut self, cwd: &Path) {
        for value in [&mut self.path, &mut self.select].into_iter().flatten() {
            if !value.is_empty() && !value.starts_with('/') && !value.contains("://") {
                *value = cwd.join(&*value).to_string_lossy().into_owned();
            }
        }
        if self.path.is_none() {
            self.path = self
                .select
                .as_deref()
                .and_then(|path| Path::new(path).parent())
                .map(|path| path.to_string_lossy().into_owned());
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    List,
    Columns,
    Grid,
    Dual,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub data: Json,
    pub view: View,
    pub rail_width: f32,
    pub columns: usize,
    pub split: f32,
    pub newest: Vec<String>,
}

fn word<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap_or("")
}

impl Settings {
    pub fn from_json(data: Json) -> Self {
        let view = match word(&data, "view") {
            "list" => View::List,
            "grid" => View::Grid,
            "columns" => View::Columns,
            "dual" => View::Dual,
            _ => View::List,
        };
        let places = data.get("places").unwrap_or(&Json::Null);
        let rail_width = if word(places, "rail") == "hidden" {
            0.
        } else {
            places
                .get("sidebarWidth")
                .and_then(Json::as_f64)
                .unwrap_or(192.) as f32
        };
        let columns = data
            .get("columnsLimit")
            .and_then(Json::as_f64)
            .unwrap_or(3.) as usize;
        let split = data
            .get("dual")
            .and_then(|dual| dual.get("split"))
            .and_then(Json::as_f64)
            .unwrap_or(5000.) as f32
            / 10_000.;
        Self {
            data,
            view,
            rail_width,
            columns,
            split,
            newest: Vec::new(),
        }
    }

    pub fn start_path(&self, home: &str, named: Option<&str>) -> String {
        if let Some(path) = named.filter(|path| !path.is_empty()) {
            return path.into();
        }
        let path = match word(&self.data, "startIn") {
            "last" => word(&self.data, "lastPath"),
            "folder" => word(&self.data, "startFolder"),
            _ => "",
        };
        if path.is_empty() {
            home.into()
        } else {
            path.into()
        }
    }

    pub fn new_tab_path(&self, here: &str, home: &str) -> String {
        match word(&self.data, "newTab") {
            "home" => home.into(),
            "start" => self.start_path(home, None),
            _ => here.into(),
        }
    }

    pub fn dual_paths(&self, start: &str, named: Option<&str>) -> ([String; 2], Option<usize>) {
        let dual = self.data.get("dual").unwrap_or(&Json::Null);
        let saved = dual.get("paths").and_then(Json::as_array).unwrap_or(&[]);
        let named = named.filter(|path| !path.is_empty());
        if saved.len() != 2 {
            let only = named.unwrap_or(start).to_owned();
            return ([only.clone(), only], named.map(|_| 0));
        }
        let mut paths = [
            saved[0].as_str().unwrap_or(start).into(),
            saved[1].as_str().unwrap_or(start).into(),
        ];
        let side = usize::from(dual.get("focus").and_then(Json::as_f64) == Some(1.));
        if let Some(named) = named {
            paths[side] = named.into();
        }
        (paths, named.map(|_| side))
    }

    /// Settings > View "Open items with": Pane.qml reads its singleClick as this same word, so only "single" opens on one click.
    pub fn single_click_open(&self) -> bool {
        word(&self.data, "openMode") == "single"
    }
}

pub fn binary() -> PathBuf {
    std::env::var_os("FLEA_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| "flea".into())
}

// Read through the existing schema owner; no alternate persistence or schema is introduced here.
pub fn load_settings() -> Result<Settings, String> {
    let output = Command::new(binary())
        .arg("--ui-state")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let data = flea::jsondoc::parse(&String::from_utf8_lossy(&output.stdout))?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut settings = Settings::from_json(data);
    // Read once on the startup worker, never on a frame: the folders that open newest first.
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    // Before the window, so a fifo or an endless file in its place reads as no file rather than a launch that never opens.
    let body =
        flea::backend::regfile::read_text(&config.join("user-dirs.dirs"), MAX_USER_DIRS_BYTES)
            .unwrap_or_default();
    settings.newest = crate::navigation::folder_state::newest_paths(&body, &home.to_string_lossy());
    Ok(settings)
}

#[derive(Clone, Debug)]
pub struct Launch {
    pub tabs: Option<crate::tabs::state::Plan>,
    pub settings: Settings,
    pub paths: [String; 2],
    pub select: [Option<String>; 2],
    pub focus: usize,
    pub unlock: Option<String>,
}

impl Launch {
    pub fn resolve(settings: Settings, args: Arguments, home: &str) -> Self {
        let tabs =
            crate::tabs::state::restore_plan(&settings.data, args.path.as_deref().unwrap_or(""));
        let start = tabs
            .as_ref()
            .and_then(|plan| plan.paths.get(plan.index))
            .cloned()
            .unwrap_or_else(|| settings.start_path(home, args.path.as_deref()));
        let (paths, launch_side) = settings.dual_paths(&start, args.path.as_deref());
        let focus = if settings.view == View::Dual {
            usize::from(
                settings
                    .data
                    .get("dual")
                    .and_then(|v| v.get("focus"))
                    .and_then(Json::as_f64)
                    == Some(1.),
            )
        } else {
            0
        };
        let paths = if settings.view == View::Dual {
            paths
        } else {
            [start, paths[1].clone()]
        };
        let mut select = [None, None];
        select[if settings.view == View::Dual {
            launch_side.unwrap_or(focus)
        } else {
            0
        }] = args.select;
        Self {
            tabs,
            settings,
            paths,
            select,
            focus,
            unlock: None,
        }
    }
}

pub fn new_window_command(path: &str) -> Command {
    let mut command = Command::new(binary());
    crate::tabs::transfer::fresh_launch(&mut command);
    command
        .args(["--gui", path])
        .env("FLEA_FRONTEND", "gpui")
        .env_remove("FLEA_PATH")
        .env_remove("FLEA_SELECT");
    command
}

#[cfg(test)]
mod startup_view_tests {
    use super::*;

    #[test]
    fn fresh_state_opens_list_and_saved_words_survive() {
        assert_eq!(
            Settings::from_json(flea::uistate::from_file("{}")).view,
            View::List
        );
        for (word, want) in [
            ("list", View::List),
            ("grid", View::Grid),
            ("columns", View::Columns),
            ("dual", View::Dual),
        ] {
            let data = flea::jsondoc::parse(&format!(r#"{{"view":"{word}"}}"#)).unwrap();
            assert_eq!(Settings::from_json(data).view, want, "saved {word}");
        }
        assert_eq!(
            Settings::from_json(flea::jsondoc::parse(r#"{"view":"bogus"}"#).unwrap()).view,
            View::List
        );
        assert_eq!(
            Settings::from_json(flea::jsondoc::parse("{}").unwrap()).view,
            View::List
        );
    }
}
