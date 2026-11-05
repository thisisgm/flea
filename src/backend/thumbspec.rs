use crate::backend::aliases::Aliases;
use crate::backend::appicon;
use std::collections::HashMap;
use std::path::PathBuf;

// Exec holds the arguments and their placeholders until argv expands them.
#[derive(Clone)]
pub struct Spec {
    pub exec: Vec<String>,
}

pub struct Thumbnailers {
    by_mime: HashMap<String, Spec>,
    stl_fallbacks: HashMap<String, Spec>,
    // Computed once when the table is built; a fail marker names the table that made it by this; see AGENTS.md "Thumbnail cache".
    fingerprint: String,
}

// The freedesktop default when XDG_DATA_DIRS is unset, which it is over a non-interactive ssh here.
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const THUMBNAILERS_SUBDIR: &str = "thumbnailers";
// The one group whose keys are honoured, because a key outside it names a program nothing may run.
const ENTRY_GROUP: &str = "[Thumbnailer Entry]";
const JPEG_MIME: &str = "image/jpeg";
pub(crate) const IIQ_MIME: &str = "image/x-phaseone-iiq";

impl Thumbnailers {
    pub fn load(aliases: &Aliases) -> Thumbnailers {
        Self::load_with(aliases, None)
    }

    // The small shared thumbnails have a native STL route. Sized raster previews
    // use the usual desktop providers and keep their own size limits.
    pub(crate) fn load_for_listing(aliases: &Aliases) -> Thumbnailers {
        let stl = std::env::current_exe()
            .ok()
            .and_then(|path| path.into_os_string().into_string().ok())
            .map(|exe| Spec {
                exec: vec![
                    exe, "--stl-thumbnail-worker".into(), "%i".into(), "%o".into(), "%s".into(),
                ],
            });
        Self::load_with(aliases, stl)
    }

    fn load_with(aliases: &Aliases, stl: Option<Spec>) -> Thumbnailers {
        let (user_dir, system_dirs) = search_path();
        let user = user_dir.as_deref().map(read_specs).unwrap_or_default();
        let system: Vec<_> = system_dirs.iter().flat_map(|dir| read_specs(dir)).collect();
        Self::from_layers(&user, &system, aliases, stl)
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
        Self::from_layers(entries, &[], aliases, None)
    }

    fn from_layers(
        user: &[(String, String)],
        system: &[(String, String)],
        aliases: &Aliases,
        stl: Option<Spec>,
    ) -> Thumbnailers {
        let mut by_mime = HashMap::new();
        add_entries(&mut by_mime, user, aliases);
        let native_keys: Vec<_> = [
            "model/stl", "application/sla", "application/vnd.ms-pki.stl",
            "model/x.stl-ascii", "model/x.stl-binary",
        ]
        .into_iter()
        .map(|mime| aliases.canonical(mime))
        .filter(|mime| !by_mime.contains_key(*mime))
        .collect();
        add_entries(&mut by_mime, system, aliases);

        let mut stl_fallbacks = HashMap::new();
        if let Some(stl) = stl {
            for key in native_keys {
                // Several aliases can canonicalize to the same key.
                if by_mime.get(key).is_some_and(|spec| spec.exec == stl.exec) {
                    continue;
                }
                if let Some(previous) = by_mime.insert(key.to_string(), stl.clone()) {
                    stl_fallbacks.insert(key.to_string(), previous);
                }
            }
        }
        let fingerprint = fingerprint_of(&by_mime, &stl_fallbacks);
        Thumbnailers { by_mime, stl_fallbacks, fingerprint }
    }

    pub(crate) fn stl_fallback(&self, mime: &str, aliases: &Aliases) -> Option<&Spec> {
        self.stl_fallbacks.get(aliases.canonical(mime))
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

fn add_entries(
    by_mime: &mut HashMap<String, Spec>,
    entries: &[(String, String)],
    aliases: &Aliases,
) {
    for (_, text) in entries {
        let (exec_line, try_exec, mimes) = fields(text);
        let Some(exec_line) = exec_line else { continue };
        let mut exec = tokenise(&exec_line);
        let Some(found) = exec.first().filter(|p| !p.is_empty()).and_then(|p| resolve(p)) else {
            continue;
        };
        // A runnable TryExec does not vouch for a missing Exec program.
        if try_exec.as_deref().is_some_and(|t| resolve(t).is_none()) {
            continue;
        }
        // The sandbox has no PATH. A later declaration does not replace the first.
        exec[0] = found;
        for mime in mimes {
            by_mime.entry(aliases.canonical(&mime).to_string())
                .or_insert_with(|| Spec { exec: exec.clone() });
        }
    }
}

fn read_specs(dir: &std::path::Path) -> Vec<(String, String)> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let entries = read.flatten().filter_map(|item| {
        let path = item.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("thumbnailer") {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        Some((path.to_string_lossy().into_owned(), text))
    }).collect();
    sorted_by_path(entries)
}

// Sample output, 16 lowercase hex characters over every (MIME type, resolved program, arguments) entry; the entries are sorted so the map's order never reaches the hash.
fn fingerprint_of(by_mime: &HashMap<String, Spec>, stl_fallbacks: &HashMap<String, Spec>) -> String {
    let mut entries: Vec<String> = by_mime.iter().map(|(mime, spec)| format!("{}\0{}", mime, spec.exec.join("\0"))).collect();
    entries.extend(stl_fallbacks.iter().map(|(mime, spec)| format!("stl-fallback\0{}\0{}", mime, spec.exec.join("\0"))));
    entries.sort();
    entries.insert(
        0,
        "embedded-field-codes-v1\nliteral-imagemagick-paths-v1".to_string(),
    ); // Retry failures made before literal ImageMagick paths.
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
fn search_path() -> (Option<PathBuf>, Vec<PathBuf>) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
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
    (home.map(|h| h.join(THUMBNAILERS_SUBDIR)), dirs)
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
