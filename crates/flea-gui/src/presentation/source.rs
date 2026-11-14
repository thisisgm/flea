use super::Fonts;
use crate::palette::{ParsedTheme, Theme};
use gpui::App;
use std::{
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub theme: Theme,
    pub parsed: ParsedTheme,
    pub fonts: Fonts,
}

impl Snapshot {
    pub fn install(&self, cx: &mut App) {
        cx.set_global(self.theme);
        cx.set_global(self.parsed);
        cx.set_global(self.fonts.clone());
    }
}

/// How many files a presentation reads: seven Omarchy and fontconfig inputs, then GTK 4's settings.ini.
pub(super) const INPUT_COUNT: usize = 8;
// GTK 4's settings.ini in `Sources::inputs`.
const GTK_SETTINGS: usize = 7;
pub(super) type Inputs = [Option<String>; INPUT_COUNT];
/// The most bytes one input may hold; a larger file is refused, never cut.
pub(super) const INPUT_CAP: usize = 65536;
pub(super) type Stamps = [Option<(u64, u64, u64, i64, i64, i64, i64, u32)>; INPUT_COUNT];

/// The files a presentation reads, and where the resolved font family is cached.
pub(super) struct Sources {
    inputs: [PathBuf; INPUT_COUNT],
    cache: PathBuf,
    // Omarchy's state folder. While it exists GTK's settings.ini is never read, so a theme swap's
    // missing shell.toml keeps today's behaviour instead of falling back to GTK's font size.
    omarchy: Option<PathBuf>,
}

impl Sources {
    #[cfg(test)]
    pub fn at(inputs: [PathBuf; INPUT_COUNT], cache: PathBuf) -> Self {
        Self {
            inputs,
            cache,
            omarchy: None,
        }
    }

    #[cfg(test)]
    pub fn with_omarchy(mut self, dir: PathBuf) -> Self {
        self.omarchy = Some(dir);
        self
    }

    /// Whether this input is read: GTK's settings.ini only without an Omarchy install.
    fn active(&self, at: usize) -> bool {
        at != GTK_SETTINGS || !self.omarchy.as_deref().is_some_and(Path::is_dir)
    }

    pub fn current() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let state = std::env::var_os("XDG_STATE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"));
        Self::from_dirs(home, config, state)
    }

    /// The inputs under these home, config and state folders, so a test needs no process environment.
    fn from_dirs(home: PathBuf, config: PathBuf, state: PathBuf) -> Self {
        let theme = home.join(".local/state/omarchy/current/theme");
        Self {
            inputs: [
                theme.join("colors.toml"),
                theme.join("shell.toml"),
                home.join(".config/omarchy/shell.toml"),
                config.join("fontconfig/fonts.conf"),
                config.join("fontconfig/conf.d/50-omarchy-input-method.conf"),
                state.join("omarchy/toggles/hypr/window-opaque.lua"),
                state.join("omarchy/toggles/hypr/window-glassy.lua"),
                // Without Omarchy's shell.toml, the text size comes from the font GTK 4 apps are given.
                config.join("gtk-4.0/settings.ini"),
            ],
            cache: state.join("flea/fc-match-monospace"),
            // The same HOME-based folder the theme is read from, whatever XDG_STATE_HOME says.
            omarchy: Some(home.join(".local/state/omarchy")),
        }
    }

    /// The folders that hold the inputs, and the one the theme folder is swapped into, for the waker to watch.
    pub fn dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self
            .inputs
            .iter()
            .filter_map(|input| input.parent())
            .map(Path::to_path_buf)
            .collect();
        dirs.extend(
            self.inputs[0]
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
        );
        dirs.sort();
        dirs.dedup();
        dirs
    }

    fn read_one(path: &Path) -> io::Result<Option<String>> {
        let mut text = String::new();
        let file = match flea::backend::regfile::open_if_regular(path, 0) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        file.take(INPUT_CAP as u64 + 1).read_to_string(&mut text)?;
        if text.len() > INPUT_CAP {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Some(text))
    }

    /// Every input or the first failure: a live change that cannot be read keeps the last complete snapshot.
    /// On an Omarchy install GTK's settings.ini is never read, so it can never hold back an Omarchy change.
    pub fn read(&self) -> io::Result<Inputs> {
        let mut inputs: Inputs = std::array::from_fn(|_| None);
        for at in (0..INPUT_COUNT).filter(|&at| self.active(at)) {
            inputs[at] = Self::read_one(&self.inputs[at])?;
        }
        Ok(inputs)
    }

    /// The launch read: an input that cannot be read counts as absent and is reported, never refused.
    pub fn read_for_launch(&self) -> (Inputs, Vec<String>) {
        let mut inputs = std::array::from_fn(|_| None);
        let mut failures = Vec::new();
        for (at, (path, input)) in self.inputs.iter().zip(&mut inputs).enumerate() {
            if !self.active(at) {
                continue;
            }
            match Self::read_one(path) {
                Ok(text) => *input = text,
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        }
        (inputs, failures)
    }

    /// What identifies each input without reading it, so an unchanged input costs one stat and no probe.
    pub fn stamps(&self) -> Stamps {
        std::array::from_fn(|at| {
            let meta = std::fs::metadata(&self.inputs[at]).ok()?;
            Some((
                meta.dev(),
                meta.ino(),
                meta.size(),
                meta.mtime(),
                meta.mtime_nsec(),
                meta.ctime(),
                meta.ctime_nsec(),
                meta.mode(),
            ))
        })
    }

    /// The snapshot for these inputs. A probe that fails keeps `fallback` for the family and is returned for the log.
    pub fn resolve(&self, input: &Inputs, fallback: &str) -> (Snapshot, Option<io::Error>) {
        self.resolve_with(input, fallback, super::face::fc_match, super::face::LAUNCH)
    }

    /// The retry's resolve after a quick probe timed out: the same inputs, with time for a cold fontconfig cache to answer.
    pub fn resolve_patient(&self, input: &Inputs, fallback: &str) -> (Snapshot, Option<io::Error>) {
        self.resolve_with(input, fallback, super::face::fc_match, super::face::PATIENT)
    }

    fn resolve_with(
        &self,
        input: &Inputs,
        fallback: &str,
        probe: impl FnOnce() -> std::process::Command,
        deadline: std::time::Duration,
    ) -> (Snapshot, Option<io::Error>) {
        let text = |at: usize| input[at].as_deref().unwrap_or("");
        let (mut theme, parsed) = Theme::from_text_observed(text(0));
        // Omarchy's shell.toml sizes the text when either copy exists; elsewhere GTK's font does.
        let shell = if input[1].is_some() || input[2].is_some() {
            text(1).to_owned()
        } else {
            flea::desktop_font::shell_text(text(7))
        };
        theme.glass = crate::window::glass::Inputs::resolve(
            text(0),
            text(1),
            text(2),
            input[5].is_some(),
            input[6].is_some(),
        );
        let (family, failure) =
            match super::face_cache::family_within(&input[3..5], &self.cache, probe, deadline) {
                Ok(family) => (family, None),
                Err(error) => (fallback.to_owned(), Some(error)),
            };
        let snapshot = Snapshot {
            theme,
            parsed,
            fonts: Fonts::from_sources(&shell, text(2), family),
        };
        (snapshot, failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn sources(inputs: [PathBuf; INPUT_COUNT]) -> Sources {
        Sources {
            inputs,
            cache: PathBuf::from("/proc/self/flea-u01-no-cache/entry"),
            omarchy: None,
        }
    }

    #[test]
    fn u01_source_read_failure_stays_distinct_from_absent_font_inputs() {
        let absent = sources(std::array::from_fn(|_| {
            PathBuf::from("/proc/self/flea-u01-absent-input")
        }));
        assert_eq!(absent.read().unwrap(), std::array::from_fn(|_| None));
        let failed = sources(std::array::from_fn(|_| PathBuf::from("/proc/self")));
        assert_eq!(
            failed.read().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let (launch, failures) = failed.read_for_launch();
        assert_eq!(launch, std::array::from_fn(|_| None));
        assert_eq!(failures.len(), INPUT_COUNT);
        let valid = sources(std::array::from_fn(|_| PathBuf::from("/proc/self/status")));
        assert!(valid
            .read()
            .unwrap()
            .iter()
            .all(|value| value.as_ref().is_some_and(|text| !text.is_empty())));
    }

    #[test]
    fn the_omarchy_gate_is_the_folder_the_theme_is_read_from() {
        let sources = Sources::from_dirs(
            PathBuf::from("/home/u"),
            PathBuf::from("/home/u/.config"),
            PathBuf::from("/var/xdg-state"),
        );
        assert_eq!(
            sources.omarchy,
            Some(PathBuf::from("/home/u/.local/state/omarchy"))
        );
        assert!(sources.inputs[0].starts_with(sources.omarchy.as_ref().unwrap()));
    }

    #[test]
    fn on_omarchy_gtk_settings_are_never_read() {
        let fixture = super::super::fixture::Fixture::new("gtk-settings");
        // A directory fails a regular-file read, as an oversized or unreadable file does.
        let paths: [PathBuf; INPUT_COUNT] = std::array::from_fn(|at| match at {
            7 => PathBuf::from("/proc/self"),
            _ => PathBuf::from("/proc/self/flea-absent"),
        });
        let omarchy = sources(paths.clone()).with_omarchy(fixture.path().to_path_buf());
        // Neither a broken settings.ini nor a theme swap's missing shell.toml reaches GTK's font.
        let read = omarchy
            .read()
            .expect("a broken settings.ini never holds back Omarchy");
        assert!(read.iter().all(Option::is_none));
        assert!(omarchy.read_for_launch().1.is_empty());
        // Without Omarchy the settings are live, so a broken file keeps the last snapshot.
        let elsewhere = sources(paths).with_omarchy(fixture.path().join("absent"));
        assert!(elsewhere.read().is_err());
        assert_eq!(elsewhere.read_for_launch().1.len(), 1);
        fixture.cleanup();
    }

    #[test]
    fn without_an_omarchy_shell_toml_the_gtk_font_sizes_the_text() {
        let absent = sources(std::array::from_fn(|_| {
            PathBuf::from("/proc/self/flea-absent")
        }));
        let mut input: Inputs = std::array::from_fn(|_| None);
        input[7] = Some("[Settings]\ngtk-font-name=Noto Sans,  10\n".into());
        assert_eq!(absent.resolve(&input, "monospace").0.fonts.base, 13);
        // An Omarchy shell.toml, even one naming no size, keeps GTK's font out.
        input[2] = Some(String::new());
        assert_eq!(absent.resolve(&input, "monospace").0.fonts.base, 12);
    }

    #[test]
    fn u01_a_failed_probe_keeps_the_fallback_family_and_a_stamp_moves_only_with_the_input() {
        let fixture = super::super::fixture::Fixture::new("stamp");
        let dir = fixture.path();
        let file = dir.join("fonts.conf");
        let one = sources(std::array::from_fn(|at| {
            if at == 3 {
                file.clone()
            } else {
                dir.join("absent")
            }
        }));
        assert_eq!(one.stamps()[3], None);
        fixture.write(&file, "one");
        let first = one.stamps();
        assert!(first[3].is_some());
        assert_eq!(first, one.stamps());
        fixture.write(&file, "three");
        assert_ne!(first, one.stamps());
        let inputs = one.read().unwrap();
        let (snapshot, failure) = one.resolve_with(
            &inputs,
            "monospace",
            || {
                let mut command = Command::new("/bin/sh");
                command.args(["-c", "exit 1"]);
                command
            },
            super::super::face::LAUNCH,
        );
        assert_eq!(snapshot.fonts.family, "monospace");
        assert_eq!(failure.unwrap().kind(), io::ErrorKind::InvalidData);
        fixture.cleanup();
    }
}

#[cfg(test)]
#[path = "../window/glass_test_bridge.rs"]
mod glass_test_bridge;
