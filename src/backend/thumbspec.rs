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
    // Computed once when the table is built; a fail marker names the table that made it by this; see AGENTS.md "Thumbnail cache".
    fingerprint: String,
}

// The freedesktop default when XDG_DATA_DIRS is unset, which it is over a non-interactive ssh here.
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const THUMBNAILERS_SUBDIR: &str = "thumbnailers";
// The one group whose keys are honoured, because a key outside it names a program nothing may run.
const ENTRY_GROUP: &str = "[Thumbnailer Entry]";
const JPEG_MIME: &str = "image/jpeg";
// The camera RAW types whose embedded JPEG is thumbnailed, by canonical name; the shared-mime-info alias image/x-panasonic-raw2 canonicalises to the RW2 entry.
const RAW_MIMES: [&str; 11] = [
    "image/x-canon-cr2", "image/x-canon-cr3", "image/x-nikon-nef", "image/x-nikon-nrw", "image/x-sony-arw", "image/x-sony-sr2",
    "image/x-adobe-dng", "image/x-fuji-raf", "image/x-olympus-orf", "image/x-panasonic-rw2", "image/x-pentax-pef",
];

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
        Thumbnailers::from_entries(&entries, aliases).with_appimage_route(aliases, appicon::available())
    }

    // An internal spec so rows.rs sets t for an AppImage; a thumbnailer file that declares the type keeps it, and without 7z there is no route.
    pub(crate) fn with_appimage_route(mut self, aliases: &Aliases, tool: bool) -> Thumbnailers {
        if tool {
            for mime in appicon::MIMES {
                self.by_mime.entry(aliases.canonical(mime).to_string()).or_insert_with(appicon::route_spec);
            }
        }
        self
    }

    pub fn from_entries(entries: &[(String, String)], aliases: &Aliases) -> Thumbnailers {
        let mut by_mime: HashMap<String, Spec> = HashMap::new();
        for (_name, text) in entries {
            let (exec_line, try_exec, mimes) = fields(text);
            let mut exec = match exec_line {
                Some(l) => tokenise(&l),
                None => continue,
            };
            let found = match exec.first().filter(|p| !p.is_empty()).and_then(|p| resolve(p)) {
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
                by_mime.entry(key).or_insert_with(|| Spec { exec: exec.clone() });
            }
        }
        let fingerprint = fingerprint_of(&by_mime);
        Thumbnailers { by_mime, fingerprint }
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    // Both sides are canonicalised, because a thumbnailer may declare either one; see AGENTS.md "MIME aliases".
    pub fn for_mime(&self, mime: &str, aliases: &Aliases) -> Option<&Spec> {
        self.by_mime.get(aliases.canonical(mime)).or_else(|| self.raw_route(mime, aliases))
    }

    // A camera RAW no thumbnailer declares is thumbnailed by the image/jpeg spec on the JPEG it embeds; none when the type is not a RAW, a thumbnailer declares it, or no image/jpeg thumbnailer is installed.
    pub fn raw_route(&self, mime: &str, aliases: &Aliases) -> Option<&Spec> {
        let canonical = aliases.canonical(mime);
        if RAW_MIMES.contains(&canonical) && !self.by_mime.contains_key(canonical) { self.by_mime.get(JPEG_MIME) } else { None }
    }
}

// Sample output, 16 lowercase hex characters over every (MIME type, resolved program, arguments) entry; the entries are sorted so the map's order never reaches the hash.
fn fingerprint_of(by_mime: &HashMap<String, Spec>) -> String {
    let mut entries: Vec<String> = by_mime.iter().map(|(mime, spec)| format!("{}\0{}", mime, spec.exec.join("\0"))).collect();
    entries.sort();
    entries.insert(0, "embedded-field-codes-v1".to_string()); // Retry failures made before embedded argv expansion.
    format!("{:016x}", crate::backend::xxh64::hash(entries.join("\n").as_bytes()))
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
        .find(|c| std::fs::metadata(c).is_ok_and(|m| m.is_file() && m.permissions().mode() & ANY_EXECUTE_BIT != 0))
        .map(|c| c.to_string_lossy().to_string())
}

// The AppImage route asks only whether a tool runs, under the same search-path rule as every thumbnailer.
pub(crate) fn is_runnable(program: &str) -> bool {
    resolve(program).is_some()
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

    // The eleven camera RAW types as the mime database names them, spelled out so the table in the code is not its own witness.
    const ELEVEN: [&str; 11] = [
        "image/x-canon-cr2", "image/x-canon-cr3", "image/x-nikon-nef", "image/x-nikon-nrw", "image/x-sony-arw", "image/x-sony-sr2",
        "image/x-adobe-dng", "image/x-fuji-raf", "image/x-olympus-orf", "image/x-panasonic-rw2", "image/x-pentax-pef",
    ];

    #[test]
    fn each_camera_raw_type_takes_the_jpeg_spec_and_only_when_one_is_installed() {
        let a = Aliases::from_str("image/x-panasonic-raw2 image/x-panasonic-rw2\n");
        let jpeg = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -j %i %o\nMimeType=image/jpeg;\n");
        for m in ELEVEN.iter().chain(&["image/x-panasonic-raw2"]) {
            assert_eq!(jpeg.raw_route(m, &a).map(|s| s.exec[1].as_str()), Some("-j"), "{m}");
            assert!(jpeg.for_mime(m, &a).is_some(), "{m}");
        }
        // Not a RAW type: the PNG, the Sony SRF the walker reads but this route does not list, and an unknown name.
        for m in ["image/png", "image/x-sony-srf", "nonsense/nothing"] {
            assert!(jpeg.raw_route(m, &a).is_none() && jpeg.for_mime(m, &a).is_none(), "{m}");
        }
        // Without an image/jpeg thumbnailer a RAW has none, as before.
        let video = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=video/mp4;\n");
        assert!(ELEVEN.iter().all(|m| video.for_mime(m, &a).is_none()));
        // A thumbnailer that declares the RAW itself still serves it when no image/jpeg one exists, and the route stays off.
        let own = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -r %i %o\nMimeType=image/x-canon-cr2;\n");
        assert_eq!(own.for_mime("image/x-canon-cr2", &a).map(|s| s.exec[1].as_str()), Some("-r"));
        assert!(own.raw_route("image/x-canon-cr2", &a).is_none());
    }

    #[test]
    fn a_thumbnailer_that_declares_a_raw_keeps_it_when_an_image_jpeg_one_is_installed_too() {
        let a = Aliases::from_str("");
        let jpeg = ("jpeg.thumbnailer".to_string(), "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -j %i %o\nMimeType=image/jpeg;\n".to_string());
        let cr2 = ("cr2.thumbnailer".to_string(), "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -r %i %o\nMimeType=image/x-canon-cr2;\n".to_string());
        let t = Thumbnailers::from_entries(&[jpeg, cr2], &a);
        // The declared CR2 is served by its own thumbnailer on the file itself, and the route stays off for it.
        assert_eq!(t.for_mime("image/x-canon-cr2", &a).map(|s| s.exec[1].as_str()), Some("-r"));
        assert!(t.raw_route("image/x-canon-cr2", &a).is_none(), "the route must stay off for a declared RAW");
        // A RAW nothing declares still takes the JPEG route.
        assert_eq!(t.for_mime("image/x-nikon-nef", &a).map(|s| s.exec[1].as_str()), Some("-j"));
        assert_eq!(t.raw_route("image/x-nikon-nef", &a).map(|s| s.exec[1].as_str()), Some("-j"));
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

    // Issue 171: the jail has no PATH, so load stores the path validation found and run never searches for a name.
    #[test]
    fn a_bare_exec_name_is_stored_as_the_path_that_validated() {
        let a = al();
        let t = one("[Thumbnailer Entry]\nExec=sh %i %o\nMimeType=image/jpeg;\n");
        let program = &t.for_mime("image/jpeg", &a).unwrap().exec[0];
        assert!(program.starts_with('/') && program.ends_with("/sh"), "the program is {}", program);
        assert!(std::path::Path::new(program).is_file());
    }

    #[test]
    fn a_bare_name_resolves_to_the_first_path_hit_that_is_a_runnable_file() {
        use std::os::unix::fs::PermissionsExt;
        let d = crate::backend::testdir::TestDir::new("thumbspec-resolve");
        let (first, second) = (d.join("first"), d.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        // The first hit is not executable, so the search goes on to the second.
        std::fs::write(first.join("tool"), b"#!/bin/sh\n").unwrap();
        std::fs::write(second.join("tool"), b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(second.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", first.display(), second.display());
        assert_eq!(resolve_in("tool", &path), Some(second.join("tool").to_string_lossy().to_string()));
        assert_eq!(resolve_in("tool", ""), None, "a name is not searched for without a PATH");
        assert_eq!(resolve_in("/definitely/not/here", &path), None);
    }

    #[test]
    fn a_relative_or_empty_path_entry_is_not_searched() {
        // A repository executable that a relative entry would find from the test's cwd.
        let here = std::env::current_dir().unwrap().join("tools");
        let absolute = here.join("flea-file-budget").to_string_lossy().to_string();
        assert_eq!(resolve_in("flea-file-budget", &here.to_string_lossy()), Some(absolute.clone()), "the control: an absolute entry finds it");
        assert_eq!(resolve_in("flea-file-budget", "tools"), None);
        assert_eq!(resolve_in("flea-file-budget", ":./tools"), None);
        assert_eq!(resolve_in("flea-file-budget", &format!("tools:{}", here.display())), Some(absolute));
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

    // Sample output, a file name and a body that declares one type served by one program.
    fn declared(mime: &str, program: &str) -> (String, String) {
        (format!("{}.thumbnailer", mime.replace('/', "-")), format!("[Thumbnailer Entry]\nExec={} %i %o %s\nMimeType={};\n", program, mime))
    }

    #[test]
    fn the_fingerprint_is_the_same_for_the_same_entries_in_any_order() {
        let a = al();
        let forward = [declared("image/png", "/bin/sh"), declared("video/mp4", "/bin/true"), declared("image/jpeg", "/bin/false"), declared("audio/flac", "/bin/true")];
        let reversed: Vec<(String, String)> = forward.iter().rev().cloned().collect();
        // Each table owns a HashMap with its own iteration order, so rebuilding is what exercises the sort.
        for _ in 0..20 {
            assert_eq!(Thumbnailers::from_entries(&forward, &a).fingerprint(), Thumbnailers::from_entries(&reversed, &a).fingerprint());
        }
    }

    #[test]
    fn the_fingerprint_changes_when_a_program_path_changes() {
        let a = al();
        let base = Thumbnailers::from_entries(&[declared("image/png", "/bin/true"), declared("video/mp4", "/bin/sh")], &a);
        let moved = Thumbnailers::from_entries(&[declared("image/png", "/bin/false"), declared("video/mp4", "/bin/sh")], &a);
        assert_ne!(base.fingerprint(), moved.fingerprint());
    }
}
