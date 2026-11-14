use crate::backend::aliases::Aliases;
use crate::backend::appicon;
use std::collections::HashMap;
use std::path::PathBuf;

// exec is the tokenised Exec line with its placeholders still in place, which argv substitutes.
pub struct Spec {
    pub exec: Vec<String>,
}

pub struct Thumbnailers {
    by_mime: HashMap<String, Spec>,
    fast_jpeg: bool,
    // Computed once when the table is built; a fail marker names the table that made it by this; see AGENTS.md "Thumbnail cache".
    fingerprint: String,
}

// The freedesktop default when XDG_DATA_DIRS is unset, which it is over a non-interactive ssh here.
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const THUMBNAILERS_SUBDIR: &str = "thumbnailers";
// The one group whose keys are honoured, because a key outside it names a program nothing may run.
const ENTRY_GROUP: &str = "[Thumbnailer Entry]";
const JPEG_MIME: &str = "image/jpeg";
const SYSTEM_GLYCIN_SPEC: &str = "/usr/share/thumbnailers/glycin-image-rs.thumbnailer";
const GLYCIN_JPEG_EXEC: [&str; 7] = [
    "/usr/bin/glycin-thumbnailer",
    "--input",
    "%u",
    "--output",
    "%o",
    "--size",
    "%s",
];
pub(crate) const IIQ_MIME: &str = "image/x-phaseone-iiq";

impl Thumbnailers {
    /// Load thumbnailers with user precedence and detect optional JPEG acceleration.
    pub fn load(aliases: &Aliases) -> Thumbnailers {
        let mut entries: Vec<(String, String)> = Vec::new();
        for dir in search_path() {
            let read = match std::fs::read_dir(&dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let mut in_dir: Vec<(String, String)> = Vec::new();
            for item in read.flatten() {
                let path = item.path();
                if path.extension().and_then(|e| e.to_str()) != Some("thumbnailer") {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    in_dir.push((path.to_string_lossy().to_string(), text));
                }
            }
            // Sorting one directory at a time, rather than the whole list, keeps the search path's own precedence intact.
            entries.append(&mut sorted_by_path(in_dir));
        }
        let vips = is_runnable(super::jpegthumb::PROGRAM);
        Thumbnailers::from_entries_with_vips(&entries, aliases, vips, resolve)
            .with_appimage_route(aliases, appicon::available())
    }

    // An internal spec so rows.rs sets t for an AppImage; a thumbnailer file that declares the type keeps it, and without 7z there is no route.
    pub(crate) fn with_appimage_route(mut self, aliases: &Aliases, tool: bool) -> Thumbnailers {
        if tool {
            for mime in appicon::MIMES {
                self.by_mime
                    .entry(aliases.canonical(mime).to_string())
                    .or_insert_with(appicon::route_spec);
            }
        }
        self
    }

    #[cfg(test)]
    pub fn from_entries(entries: &[(String, String)], aliases: &Aliases) -> Thumbnailers {
        Self::from_entries_with_vips(entries, aliases, false, resolve)
    }

    /// Build the effective provider table and detect the shipped Glycin JPEG entry.
    pub(super) fn from_entries_with_vips(
        entries: &[(String, String)],
        aliases: &Aliases,
        vips: bool,
        resolve: impl Fn(&str) -> Option<String>,
    ) -> Thumbnailers {
        let mut by_mime: HashMap<String, Spec> = HashMap::new();
        let mut fast_jpeg = false;
        for (name, text) in entries {
            let (exec_line, try_exec, mimes) = fields(text);
            let mut exec = match exec_line {
                Some(l) => tokenise(&l),
                None => continue,
            };
            let found = match exec
                .first()
                .filter(|p| !p.is_empty())
                .and_then(|p| resolve(p))
            {
                Some(f) => f,
                None => continue,
            };
            // Both are validated: a TryExec that resolves must not vouch for an Exec program that does not.
            if try_exec.as_deref().is_some_and(|t| resolve(t).is_none()) {
                continue;
            }
            // The jail has no PATH, so the program is stored as the path validation found; see AGENTS.md "Thumbnailer specs".
            exec[0] = found;
            for mime in mimes {
                let key = aliases.canonical(&mime).to_string();
                // corner: the first file to declare a MIME type keeps it, and load sorts each directory by path so that winner is fixed; see AGENTS.md "Thumbnailer specs".
                by_mime.entry(key).or_insert_with_key(|key| {
                    if key == JPEG_MIME {
                        fast_jpeg = vips && name == SYSTEM_GLYCIN_SPEC && exec == GLYCIN_JPEG_EXEC;
                    }
                    Spec { exec: exec.clone() }
                });
            }
        }
        let fingerprint = fingerprint_of(&by_mime, fast_jpeg);
        Thumbnailers {
            by_mime,
            fast_jpeg,
            fingerprint,
        }
    }

    /// Report whether the configured JPEG thumbnailer may use the optional accelerator.
    pub(super) fn can_speed_up_jpeg(&self, mime: &str, aliases: &Aliases) -> bool {
        self.fast_jpeg && aliases.canonical(mime) == JPEG_MIME
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    // Both sides are canonicalised, because a thumbnailer may declare either one; see AGENTS.md "MIME aliases".
    pub fn for_mime(&self, mime: &str, aliases: &Aliases) -> Option<&Spec> {
        self.by_mime
            .get(aliases.canonical(mime))
            .or_else(|| self.raw_route(mime, aliases))
    }

    // A camera RAW no thumbnailer declares is thumbnailed by the image/jpeg spec on the JPEG it embeds; none when the type is not a RAW, a thumbnailer declares it, or no image/jpeg thumbnailer is installed.
    pub fn raw_route(&self, mime: &str, aliases: &Aliases) -> Option<&Spec> {
        let canonical = aliases.canonical(mime);
        let raw = flea::rawname::FORMATS
            .iter()
            .any(|(_, mime)| aliases.canonical(mime) == canonical);
        if raw && !self.by_mime.contains_key(canonical) {
            self.by_mime.get(if canonical == IIQ_MIME {
                "image/tiff"
            } else {
                JPEG_MIME
            })
        } else {
            None
        }
    }
}

/// Hash the effective thumbnailer table so cached failures can be retried after routing changes.
fn fingerprint_of(by_mime: &HashMap<String, Spec>, fast_jpeg: bool) -> String {
    let mut entries: Vec<String> = by_mime
        .iter()
        .map(|(mime, spec)| format!("{}\0{}", mime, spec.exec.join("\0")))
        .collect();
    entries.sort();
    entries.insert(
        0,
        "embedded-field-codes-v1\nliteral-imagemagick-paths-v1".to_string(),
    ); // Retry failures made before embedded argv expansion.
    if fast_jpeg {
        entries.push("libvips-jpeg-v1".to_string());
    }
    format!(
        "{:016x}",
        crate::backend::xxh64::hash(entries.join("\n").as_bytes())
    )
}

// read_dir order is unspecified by POSIX, so a directory's files are ordered by path and the winner of a duplicate declaration is the same on every box.
fn sorted_by_path(mut entries: Vec<(String, String)>) -> Vec<(String, String)> {
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

// XDG_DATA_HOME comes first, so a user's own thumbnailer overrides a system one, as the freedesktop spec orders it.
fn search_path() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(h) = home {
        dirs.push(h.join(THUMBNAILERS_SUBDIR));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let data_dirs = if data_dirs.is_empty() {
        DEFAULT_DATA_DIRS.to_string()
    } else {
        data_dirs
    };
    for d in data_dirs.split(':') {
        if !d.is_empty() {
            dirs.push(PathBuf::from(d).join(THUMBNAILERS_SUBDIR));
        }
    }
    dirs
}

// Sample input, a .thumbnailer file: "[Thumbnailer Entry]", then "TryExec=prog", "Exec=prog -i %i -o %o -s %s", "MimeType=video/mp4;video/webm;".
fn fields(text: &str) -> (Option<String>, Option<String>, Vec<String>) {
    let mut exec = None;
    let mut try_exec = None;
    let mut mimes = Vec::new();
    let mut in_entry = false;
    for line in text.lines() {
        // Only the [Thumbnailer Entry] group names a program to run, so a key in any other group is not read at all.
        if line.trim_end().starts_with('[') {
            in_entry = line.trim_end() == ENTRY_GROUP;
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = line.strip_prefix("Exec=") {
            exec = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("TryExec=") {
            try_exec = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("MimeType=") {
            for m in v.trim().split(';') {
                if !m.is_empty() {
                    mimes.push(m.to_string());
                }
            }
        }
    }
    (exec, try_exec, mimes)
}

// Desktop Entry quoting: a double-quoted token may hold spaces, and a backslash escapes the next byte inside one.
fn tokenise(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    let mut started = false;
    for ch in line.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escaped = true,
            '"' => {
                in_quotes = !in_quotes;
                started = true;
            }
            ' ' | '\t' if !in_quotes => {
                if started || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            _ => {
                cur.push(ch);
                started = true;
            }
        }
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    out
}

// A program that is not a regular executable file is refused, because the search path includes a user-writable directory; answers the path that passed.
fn resolve(program: &str) -> Option<String> {
    resolve_in(program, &std::env::var("PATH").unwrap_or_default())
}

// Split from resolve() so a test can name the PATH; sample input: "tool" with "/home/u/.local/bin:/usr/bin" answers "/home/u/.local/bin/tool" when that file is executable, and an empty or relative entry is skipped.
fn resolve_in(program: &str, path: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    const ANY_EXECUTE_BIT: u32 = 0o111;
    let candidates: Vec<PathBuf> = if program.contains('/') {
        vec![PathBuf::from(program)]
    } else {
        path.split(':')
            .filter(|d| d.starts_with('/'))
            .map(|d| PathBuf::from(d).join(program))
            .collect()
    };
    candidates
        .into_iter()
        .find(|c| {
            std::fs::metadata(c)
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & ANY_EXECUTE_BIT != 0)
        })
        .map(|c| c.to_string_lossy().to_string())
}

// The AppImage route asks only whether a tool runs, under the same search-path rule as every thumbnailer.
pub(crate) fn is_runnable(program: &str) -> bool {
    resolve(program).is_some()
}

#[cfg(test)]
#[path = "thumbspec_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "raw_gap64_tests.rs"]
mod gap64_tests;

#[cfg(test)]
#[path = "thumbspec_jpeg_tests.rs"]
mod jpeg_tests;
