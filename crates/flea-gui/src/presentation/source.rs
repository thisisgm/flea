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

pub(super) type Inputs = [Option<String>; 7];
/// The most bytes one input may hold; a larger file is refused, never cut.
pub(super) const INPUT_CAP: usize = 65536;
pub(super) type Stamps = [Option<(u64, u64, u64, i64, i64, i64, i64, u32)>; 7];

/// The seven files a presentation reads, and where the resolved font family is cached.
pub(super) struct Sources {
    inputs: [PathBuf; 7],
    cache: PathBuf,
}

impl Sources {
    #[cfg(test)]
    pub fn at(inputs: [PathBuf; 7], cache: PathBuf) -> Self {
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
        let (mut theme, parsed) = Theme::from_text_observed(text(0));
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

    fn sources(inputs: [PathBuf; 7]) -> Sources {
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
        assert_eq!(
            absent.read().unwrap(),
            [None, None, None, None, None, None, None]
        );
        let failed = sources(std::array::from_fn(|_| PathBuf::from("/proc/self")));
        assert_eq!(
            failed.read().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let (launch, failures) = failed.read_for_launch();
        assert_eq!(launch, [None, None, None, None, None, None, None]);
        assert_eq!(failures.len(), 7);
        let valid = sources(std::array::from_fn(|_| PathBuf::from("/proc/self/status")));
        assert!(valid
            .read()
            .unwrap()
            .iter()
            .all(|value| value.as_ref().is_some_and(|text| !text.is_empty())));
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
