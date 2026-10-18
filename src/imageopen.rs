// The image gallery's default-handler capability and shared argv bound, never a viewer choice.
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MAX_IMAGES: usize = 4096;
const MAX_BYTES: usize = 512 * 1024;
const RESERVE_BYTES: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 32 * 1024;
const URI_ARG_OVERHEAD_BYTES: usize = 16;
const URI_ESCAPE_MULTIPLIER: usize = 3;
const MIN_ARG_MAX_BYTES: usize = 128 * 1024;
const SC_ARG_MAX: i32 = 0;
extern "C" {
    fn sysconf(name: i32) -> std::os::raw::c_long;
}

pub struct Budget {
    bytes: usize,
    count: usize,
}
impl Budget {
    pub fn new() -> Self {
        // Linux _SC_ARG_MAX is 0; the fallback is Linux's minimum, never the usual 2 MiB guess.
        let maximum = usize::try_from(unsafe { sysconf(SC_ARG_MAX) }).unwrap_or(MIN_ARG_MAX_BYTES);
        let environment: usize = std::env::vars_os()
            .map(|(key, value)| {
                key.as_bytes().len() + value.as_bytes().len() + 2 + std::mem::size_of::<usize>()
            })
            .sum();
        Self::with_bytes(
            maximum
                .saturating_sub(environment + RESERVE_BYTES)
                .min(MAX_BYTES),
        )
    }
    fn with_bytes(bytes: usize) -> Self {
        Self { bytes, count: 0 }
    }
    pub fn take(&mut self, path: &Path) -> bool {
        // %U may escape every UTF-8 byte as three ASCII bytes; include file://, NUL and argv pointer.
        let length = path.as_os_str().as_bytes().len();
        if length > MAX_PATH_BYTES {
            return false;
        }
        let cost = length
            .saturating_mul(URI_ESCAPE_MULTIPLIER)
            .saturating_add(URI_ARG_OVERHEAD_BYTES);
        if self.count >= MAX_IMAGES || cost > self.bytes {
            return false;
        }
        self.bytes -= cost;
        self.count += 1;
        true
    }
}

// Desktop Entry string escaping precedes Exec quoting, so each backslash is decoded exactly once.
// Sample raw value: viewer\s%F becomes "viewer %F" before Exec quoting.
fn decode_exec_value(raw: &str) -> Option<String> {
    let mut decoded = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        let ch = if ch == '\\' {
            match chars.next()? {
                's' => ' ',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                _ => return None,
            }
        } else {
            ch
        };
        decoded.push(ch);
    }
    Some(decoded)
}

// One unquoted, standalone list code is the desktop contract; literal %% and action groups are not.
// Sample raw values: viewer "a\sb" %F accepts a gallery; viewer "%F" does not.
pub fn takes_many(exec: &str) -> bool {
    let Some(exec) = decode_exec_value(exec) else {
        return false;
    };
    let mut chars = exec.char_indices().peekable();
    let mut quoted = false;
    let mut lists = 0;
    while let Some((at, ch)) = chars.next() {
        if ch == '\\' {
            if chars.next().is_none() {
                return false;
            }
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            continue;
        }
        if ch != '%' {
            continue;
        }
        let Some((end, code)) = chars.next() else {
            return false;
        };
        match code {
            '%' | 'i' | 'c' | 'k' => {}
            'F' | 'U' => {
                let before = exec[..at]
                    .chars()
                    .next_back()
                    .map_or(true, char::is_whitespace);
                let after = exec[end + 1..]
                    .chars()
                    .next()
                    .map_or(true, char::is_whitespace);
                if quoted || !before || !after {
                    return false;
                }
                lists += 1;
            }
            _ => return false,
        }
    }
    !quoted && lists == 1
}

// GIO chooses the opened file's MIME default and expands Exec itself, including Terminal=true.
pub fn launcher(target: &Path, paths: &[String]) -> Option<Command> {
    if paths.len() < 2 {
        return None;
    }
    let mime = crate::backend::menu_registry::content_type(
        &Default::default(),
        target,
        &Default::default(),
    )
    .ok()?;
    if !mime.starts_with("image/") {
        return None;
    }
    let id = crate::desktopid::default_handler_gio(&mime);
    let app = crate::desktopid::resolve(&id)?;
    let exec = crate::backend::menu_registry::desktop_exec_raw(&app).ok()??;
    if !takes_many(&exec) {
        return None;
    }
    let mut budget = Budget::new();
    let mut chosen = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        // Only the explicitly opened target is canonicalized; companion names cost no per-file stat.
        let path = if index == 0 {
            target.to_path_buf()
        } else {
            PathBuf::from(path)
        };
        if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
            return None;
        }
        if !budget.take(&path) {
            break;
        }
        chosen.push(path);
    }
    if chosen.len() < 2 {
        return None;
    }
    let mut launcher = Command::new("gio");
    launcher.arg("launch").arg(app).args(chosen);
    Some(launcher)
}

#[cfg(test)]
#[path = "imageopen_tests.rs"]
mod handoff_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_string_value_escapes_decode_exactly_once() {
        for (raw, expected) in [
            (r"\s", " "),
            (r"\n", "\n"),
            (r"\t", "\t"),
            (r"\r", "\r"),
            (r"\\", "\\"),
            (r"\\s", r"\s"),
        ] {
            assert_eq!(decode_exec_value(raw).as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn field_codes_distinguish_lists_from_single_paths_and_literals() {
        for exec in [
            "viewer %F",
            "viewer %U",
            "\"/opt/My Viewer\" --flag %F",
            "viewer %i %c %k %F",
        ] {
            assert!(takes_many(exec), "{exec}");
        }
        for exec in [
            "viewer %f",
            "viewer %u",
            "viewer",
            "viewer %%F",
            "viewer \"%F\"",
            "viewer \"a %F b\"",
            "viewer --files=%F",
            "viewer %F %f",
            "viewer %F %U",
            "viewer %F %",
            "viewer %Z %F",
            r"viewer \q %F",
            "viewer %F\\",
            "viewer %F\\\\",
        ] {
            assert!(!takes_many(exec), "{exec}");
        }
    }
    #[test]
    fn ten_thousand_paths_fit_the_count_and_uri_byte_bounds() {
        let mut budget = Budget::with_bytes(MAX_BYTES);
        let chosen: Vec<_> = (0..10_000)
            .map(|i| format!("/images/image-{i:05}.png"))
            .take_while(|p| budget.take(Path::new(p)))
            .collect();
        assert_eq!(chosen.len(), MAX_IMAGES);
        assert!(chosen.iter().map(|p| p.len() * 3 + 16).sum::<usize>() <= MAX_BYTES);
        let mut budget = Budget::with_bytes(128);
        assert!(budget.take(Path::new("/a.png")));
        assert!(!budget.take(Path::new(&format!("/{}.png", "x".repeat(100)))));
        assert_eq!(budget.count, 1);
        assert!(
            !Budget::with_bytes(MAX_BYTES).take(Path::new(&format!("/{}", "x".repeat(33 * 1024))))
        );
    }
}
