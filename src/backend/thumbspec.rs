use crate::backend::aliases::Aliases;
use std::collections::HashMap;
use std::path::PathBuf;

// exec is the tokenised Exec line with its placeholders still in place, which argv substitutes.
pub struct Spec {
    pub exec: Vec<String>,
}

pub struct Thumbnailers {
    by_mime: HashMap<String, Spec>,
}

// The freedesktop default when XDG_DATA_DIRS is unset, which it is over a non-interactive ssh here.
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const THUMBNAILERS_SUBDIR: &str = "thumbnailers";
// The one group whose keys are honoured, because a key outside it names a program nothing may run.
const ENTRY_GROUP: &str = "[Thumbnailer Entry]";

impl Thumbnailers {
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
        Thumbnailers::from_entries(&entries, aliases)
    }

    pub fn from_entries(entries: &[(String, String)], aliases: &Aliases) -> Thumbnailers {
        let mut by_mime: HashMap<String, Spec> = HashMap::new();
        for (_name, text) in entries {
            let (exec_line, try_exec, mimes) = fields(text);
            let exec = match exec_line {
                Some(l) => tokenise(&l),
                None => continue,
            };
            let program = match exec.first() {
                Some(p) if !p.is_empty() => p.as_str(),
                _ => continue,
            };
            // Both are validated: a TryExec that resolves must not vouch for an Exec program that does not.
            if !is_runnable(program) || try_exec.as_deref().is_some_and(|t| !is_runnable(t)) {
                continue;
            }
            for mime in mimes {
                let key = aliases.canonical(&mime).to_string();
                // corner: the first file to declare a MIME type keeps it, and load sorts each directory by path so that winner is fixed; see AGENTS.md "Thumbnailer specs".
                by_mime.entry(key).or_insert_with(|| Spec { exec: exec.clone() });
            }
        }
        Thumbnailers { by_mime }
    }

    // Both sides are canonicalised, because a thumbnailer may declare either one; see AGENTS.md "MIME aliases".
    pub fn for_mime(&self, mime: &str, aliases: &Aliases) -> Option<&Spec> {
        self.by_mime.get(aliases.canonical(mime))
    }
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
    let data_dirs = if data_dirs.is_empty() { DEFAULT_DATA_DIRS.to_string() } else { data_dirs };
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

// A program that is not a regular executable file is refused, because the search path includes a user-writable directory.
fn is_runnable(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const ANY_EXECUTE_BIT: u32 = 0o111;
    let candidates: Vec<PathBuf> = if program.contains('/') {
        vec![PathBuf::from(program)]
    } else {
        let path = std::env::var("PATH").unwrap_or_default();
        path.split(':')
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join(program))
            .collect()
    };
    for c in candidates {
        if let Ok(meta) = std::fs::metadata(&c) {
            if meta.is_file() && meta.permissions().mode() & ANY_EXECUTE_BIT != 0 {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::aliases::Aliases;

    fn al() -> Aliases {
        Aliases::from_str("image/heic image/heif\nvideo/x-matroska video/matroska\n")
    }

    fn one(body: &str) -> Thumbnailers {
        Thumbnailers::from_entries(&[("t.thumbnailer".to_string(), body.to_string())], &al())
    }

    #[test]
    fn a_declared_type_finds_its_spec() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -i %i -o %o -s %s\nMimeType=video/mp4;video/webm;\n");
        assert!(t.for_mime("video/mp4", &a).is_some());
        assert!(t.for_mime("video/webm", &a).is_some());
        assert!(t.for_mime("image/jpeg", &a).is_none());
    }

    #[test]
    fn either_side_of_an_alias_pair_matches() {
        let a = al();
        // The declaration uses the alias and the query uses the canonical name.
        let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=video/x-matroska;\n");
        assert!(t.for_mime("video/matroska", &a).is_some());
        assert!(t.for_mime("video/x-matroska", &a).is_some());

        // And the other direction: the declaration uses the canonical name and the query uses the alias.
        let t2 = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %u %o\nMimeType=image/heif;\n");
        assert!(t2.for_mime("image/heic", &a).is_some());
        assert!(t2.for_mime("image/heif", &a).is_some());
    }

    #[test]
    fn a_missing_try_exec_is_allowed_and_the_exec_program_is_validated_instead() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nExec=/bin/sh %i %o\nMimeType=application/x-xoj;\n");
        assert!(t.for_mime("application/x-xoj", &a).is_some());
    }

    #[test]
    fn an_unresolvable_try_exec_is_refused() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nTryExec=/definitely/not/here\nExec=/definitely/not/here %i %o\nMimeType=image/jpeg;\n");
        assert!(t.for_mime("image/jpeg", &a).is_none());
    }

    #[test]
    fn a_try_exec_that_resolves_does_not_vouch_for_a_missing_exec_program() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nTryExec=/bin/true\nExec=/definitely/not/here %i %o\nMimeType=image/jpeg;\n");
        assert!(t.for_mime("image/jpeg", &a).is_none());
    }

    #[test]
    fn a_key_outside_the_thumbnailer_entry_group_is_not_read() {
        let a = al();
        // The real Exec is inside the group; the one after it belongs to another group and must not win.
        let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=image/jpeg;\n[Other]\nExec=/definitely/not/here %i %o\n");
        assert_eq!(t.for_mime("image/jpeg", &a).unwrap().exec[0], "/bin/sh");
        // And a whole entry that never opens its group declares nothing at all.
        let t2 = one("TryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=image/jpeg;\n");
        assert!(t2.for_mime("image/jpeg", &a).is_none());
    }

    #[test]
    fn a_directory_as_the_program_is_refused() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nTryExec=/tmp\nExec=/tmp %i %o\nMimeType=image/jpeg;\n");
        assert!(t.for_mime("image/jpeg", &a).is_none());
    }

    #[test]
    fn a_file_with_no_trailing_newline_keeps_its_last_field() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=video/mp4;video/webm;");
        assert!(t.for_mime("video/webm", &a).is_some());
    }

    #[test]
    fn quoted_exec_tokens_survive_a_space() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=\"/bin/sh\" --flag \"two words\" %i %o\nMimeType=image/jpeg;\n");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        assert_eq!(s.exec[0], "/bin/sh");
        assert_eq!(s.exec[1], "--flag");
        assert_eq!(s.exec[2], "two words");
    }

    // Sample input, the MimeType field of a real /usr/share/thumbnailers file, shortened to the types asserted here; the programs stand in for evince, ffmpegthumbnailer and glycin because from_entries validates each against the live filesystem, so a real thumbnailer name asserts what the build machine has installed (issue 50).
    fn shipped() -> Thumbnailers {
        let evince = "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -s %s %u %o\nMimeType=application/pdf;image/tiff\n";
        let glycin = "[Thumbnailer Entry]\nTryExec=/bin/false\nExec=/bin/false --input %u --output %o --size %s\nMimeType=image/jpeg;image/heif;image/tiff\n";
        let ffmpeg = "[Thumbnailer Entry]\nTryExec=/bin/true\nExec=/bin/true -i %i -o %o -s %s -f\nMimeType=video/mp4;video/webm;video/matroska\n";
        Thumbnailers::from_entries(
            &[
                ("/usr/share/thumbnailers/evince.thumbnailer".to_string(), evince.to_string()),
                ("/usr/share/thumbnailers/ffmpegthumbnailer.thumbnailer".to_string(), ffmpeg.to_string()),
                ("/usr/share/thumbnailers/glycin-image-rs.thumbnailer".to_string(), glycin.to_string()),
            ],
            &al(),
        )
    }

    #[test]
    fn the_heaviest_shipped_field_parses() {
        let a = al();
        let t = shipped();
        assert!(t.for_mime("image/jpeg", &a).is_some());
        assert!(t.for_mime("video/mp4", &a).is_some());
        assert!(t.for_mime("video/webm", &a).is_some());
        // The two alias cases Plan 3 found, from opposite sides.
        assert!(t.for_mime("image/heif", &a).is_some());
        assert!(t.for_mime("video/matroska", &a).is_some());
        assert!(t.for_mime("nonsense/nothing", &a).is_none());
    }

    #[test]
    fn a_type_two_files_declare_goes_to_the_first_in_path_order() {
        // Both evince.thumbnailer and glycin-image-rs.thumbnailer declare image/tiff, and deleting the sort reddens this only while read_dir happens to return glycin first, which POSIX does not promise.
        assert_eq!(shipped().for_mime("image/tiff", &al()).unwrap().exec[0], "/bin/sh");
    }

    // The only test here that reads this box's own directory, and it asserts the read rather than which thumbnailers are installed, because not one of them is even an optional dependency.
    #[test]
    fn the_live_directory_is_read_without_a_panic() {
        let a = Aliases::load();
        assert!(Thumbnailers::load(&a).for_mime("nonsense/nothing", &a).is_none());
    }

    #[test]
    fn a_directorys_files_are_ordered_by_path_whatever_read_dir_returned() {
        let unsorted = vec![
            ("/usr/share/thumbnailers/glycin-image-rs.thumbnailer".to_string(), String::new()),
            ("/usr/share/thumbnailers/evince.thumbnailer".to_string(), String::new()),
        ];
        assert_eq!(sorted_by_path(unsorted)[0].0, "/usr/share/thumbnailers/evince.thumbnailer");
    }
}
