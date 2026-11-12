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

/// How many files a presentation reads: seven Omarchy and fontconfig inputs, then three GTK 4 stylesheets.
pub(super) const INPUT_COUNT: usize = 10;
pub(super) type Inputs = [Option<String>; INPUT_COUNT];
/// The most bytes one input may hold; a larger file is refused, never cut.
pub(super) const INPUT_CAP: usize = 65536;
pub(super) type Stamps = [Option<(u64, u64, u64, i64, i64, i64, i64, u32)>; INPUT_COUNT];

/// The files a presentation reads, and where the resolved font family is cached.
pub(super) struct Sources {
    inputs: [PathBuf; INPUT_COUNT],
    cache: PathBuf,
}

impl Sources {
    #[cfg(test)]
    pub fn at(inputs: [PathBuf; INPUT_COUNT], cache: PathBuf) -> Self {
        Self { inputs, cache }
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
                // Without an Omarchy theme the palette comes from what GTK 4 apps are given:
                // gtk.css, the colors.css a KDE or theme export imports into it, and the
                // wallpaper palette DankMaterialShell writes beside them.
                config.join("gtk-4.0/gtk.css"),
                config.join("gtk-4.0/colors.css"),
                config.join("gtk-4.0/dank-colors.css"),
            ],
            cache: state.join("flea/fc-match-monospace"),
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
    pub fn read(&self) -> io::Result<Inputs> {
        let mut inputs = std::array::from_fn(|_| None);
        for (path, input) in self.inputs.iter().zip(&mut inputs) {
            *input = Self::read_one(path)?;
        }
        Ok(inputs)
    }

    /// The launch read: an input that cannot be read counts as absent and is reported, never refused.
    pub fn read_for_launch(&self) -> (Inputs, Vec<String>) {
        let mut inputs = std::array::from_fn(|_| None);
        let mut failures = Vec::new();
        for (path, input) in self.inputs.iter().zip(&mut inputs) {
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
        // An Omarchy theme names the palette; elsewhere the GTK sheets do, the imported colors.css
        // first, gtk.css over it, and the DankMaterialShell palette last.
        let desktop;
        let colours = match input[0].as_deref() {
            Some(omarchy) => omarchy,
            None => {
                desktop = flea::desktop_palette::colours_text(&[text(8), text(7), text(9)]);
                &desktop
            }
        };
        let (mut theme, parsed) = Theme::from_text_observed(colours);
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
            fonts: Fonts::from_sources(text(1), text(2), family),
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
    fn without_an_omarchy_theme_the_gtk_sheets_paint_and_dms_wins() {
        let absent = sources(std::array::from_fn(|_| {
            PathBuf::from("/proc/self/flea-absent")
        }));
        let mut input: Inputs = std::array::from_fn(|_| None);
        input[8] = Some("@define-color theme_bg_color_breeze #202326;\n@define-color accent_color_breeze #3daee9;".into());
        input[9] = Some("@define-color accent_bg_color #c4c0ff;".into());
        let (snapshot, _) = absent.resolve(&input, "monospace");
        let want = Theme::from_text("background = '#202326'\naccent = '#c4c0ff'\n");
        assert!(snapshot.theme.background == want.background);
        assert!(snapshot.theme.accent == want.accent);
        // An Omarchy colors.toml, even one naming nothing, keeps the GTK sheets out.
        input[0] = Some(String::new());
        let (omarchy, _) = absent.resolve(&input, "monospace");
        assert!(omarchy.theme.background == Theme::from_text("").background);
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
