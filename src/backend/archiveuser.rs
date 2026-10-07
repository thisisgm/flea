// Compress formats the operator adds without a Flea release: one line each in
// $XDG_CONFIG_HOME/flea/compressors, read once by Formats::probe. See AGENTS.md "User compress formats".
// Sample file:
//   # id   program                    arguments
//   zjx    /home/me/.local/bin/zjx    pack --output {out} --base {dir} -- {names}
use std::path::{Path, PathBuf};

pub const FILE: &str = "compressors";
// A handful is the use; a cap keeps a pasted log from becoming a submenu of hundreds.
const MAX_FORMATS: usize = 16;
// The id becomes the archive's extension and the submenu's ".id" label, so it stays short.
const MAX_ID: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct UserFormat {
    pub id: String,
    // Canonical and absolute: the jail binds exactly this file read-only and executes it.
    pub program: PathBuf,
    args: Vec<String>,
}

impl UserFormat {
    // {out} and {dir} are replaced wherever they appear in an argument; {names} and {paths} are whole
    // arguments that become one argument per selected item. Nothing passes through a shell.
    pub fn argv(&self, dest: &Path, parent: &Path, names: &[String]) -> Vec<String> {
        let out = dest.to_string_lossy();
        let dir = parent.to_string_lossy();
        let mut a = Vec::with_capacity(self.args.len() + names.len() + 1);
        a.push(self.program.to_string_lossy().to_string());
        for arg in &self.args {
            match arg.as_str() {
                "{names}" => a.extend(names.iter().cloned()),
                "{paths}" => a.extend(names.iter().map(|n| parent.join(n).to_string_lossy().to_string())),
                _ => a.push(arg.replace("{out}", &out).replace("{dir}", &dir)),
            }
        }
        a
    }
}

// The archive jail (sandbox::wrap_archive) plus one read-only bind: the format's own executable, which may
// live outside /usr, such as ~/.local/bin. Nothing else of the home becomes visible.
pub fn wrap(inner: &[String], input: &Path, out: &Path, program: &Path) -> Vec<String> {
    let mut a = crate::backend::sandbox::wrap_archive(&[], input, out);
    let program = program.to_string_lossy().to_string();
    a.extend(["--ro-bind".to_string(), program.clone(), program]);
    a.extend_from_slice(inner);
    a
}

// The file under the config home, or None when there is no config home at all.
pub fn path() -> Option<PathBuf> {
    crate::userfile::config_home().ok().map(|d| d.join("flea").join(FILE))
}

// No file is the ordinary case and says nothing; every refused line is said once on stderr, because a
// format that silently never appears in the submenu is a defect the operator cannot see.
pub fn load(reserved: &[&str]) -> Vec<UserFormat> {
    let Some(file) = path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&file) else { return Vec::new() };
    let (formats, refused) = parse(&text, reserved, executable);
    for line in refused {
        eprintln!("flea: {}: {}", file.display(), line);
    }
    formats
}

// Split from load() so the rules are testable without a config home or real programs; `resolve`
// answers the canonical path of a usable executable. Built-in ids win over a user line of the same id.
pub fn parse(text: &str, reserved: &[&str], resolve: impl Fn(&Path) -> Option<PathBuf>) -> (Vec<UserFormat>, Vec<String>) {
    let mut formats: Vec<UserFormat> = Vec::new();
    let mut refused = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let id = words.next().unwrap_or_default();
        let program = words.next().unwrap_or_default();
        let args: Vec<String> = words.map(str::to_string).collect();
        let why = if !valid_id(id) {
            Some(format!("\"{}\" is not a format id: lowercase letters and digits, dot separated, at most {}", id, MAX_ID))
        } else if reserved.contains(&id) || formats.iter().any(|f| f.id == id) {
            Some(format!("\"{}\" is already a format", id))
        } else if formats.len() == MAX_FORMATS {
            Some(format!("only the first {} formats are read", MAX_FORMATS))
        } else if !program.starts_with('/') {
            Some(format!("the program for \"{}\" must be an absolute path", id))
        } else if args.iter().filter(|a| a.contains("{out}")).count() != 1 {
            Some(format!("\"{}\" must name {{out}}, the archive to write, exactly once", id))
        } else if args.iter().filter(|a| *a == "{names}" || *a == "{paths}").count() != 1 {
            Some(format!("\"{}\" must take the selection as one whole {{names}} or {{paths}} argument", id))
        } else {
            None
        };
        if let Some(why) = why {
            refused.push(format!("line {}: {}", i + 1, why));
            continue;
        }
        match resolve(Path::new(program)) {
            Some(program) => formats.push(UserFormat { id: id.to_string(), program, args }),
            None => refused.push(format!("line {}: {} is not an executable file", i + 1, program)),
        }
    }
    (formats, refused)
}

// Sample input: "zjx", "tar.lz4". The id is a file extension, so no slash, space or leading dot.
fn valid_id(id: &str) -> bool {
    id.len() <= MAX_ID
        && id.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
}

// The canonical path of a regular file with an execute bit, so a symlink in ~/.local/bin binds its target.
fn executable(program: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let real = std::fs::canonicalize(program).ok()?;
    let meta = std::fs::metadata(&real).ok()?;
    (meta.is_file() && meta.permissions().mode() & 0o111 != 0).then_some(real)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(p: &Path) -> Option<PathBuf> {
        Some(p.to_path_buf())
    }

    #[test]
    fn a_line_becomes_a_format_whose_argv_fills_every_placeholder() {
        let text = "# id program args\n\nzjx /opt/zjx/bin/zjx pack --output {out} --base {dir} -- {names}\n";
        let (formats, refused) = parse(text, &["zip"], found);
        assert!(refused.is_empty(), "{:?}", refused);
        assert_eq!(formats.len(), 1);
        let names = ["a b.txt".to_string(), "-dash".to_string()];
        let argv = formats[0].argv(Path::new("/w/.flea-arc/archive.zjx"), Path::new("/w"), &names);
        assert_eq!(argv, ["/opt/zjx/bin/zjx", "pack", "--output", "/w/.flea-arc/archive.zjx", "--base", "/w",
                          "--", "a b.txt", "-dash"]);
    }

    #[test]
    fn paths_are_absolute_and_out_is_replaced_inside_an_argument() {
        let (formats, _) = parse("lz /usr/bin/lzt -o{out} {paths}", &[], found);
        let argv = formats[0].argv(Path::new("/w/x.lz"), Path::new("/w"), &["a".to_string(), "b".to_string()]);
        assert_eq!(argv, ["/usr/bin/lzt", "-o/w/x.lz", "/w/a", "/w/b"]);
    }

    // Every refusal names its line, and a refused line never stops the lines after it.
    #[test]
    fn a_bad_line_is_refused_with_its_reason_and_the_rest_still_load() {
        let text = "Bad /usr/bin/t {out} {names}\n\
                    zip /usr/bin/t {out} {names}\n\
                    rel bin/t {out} {names}\n\
                    noout /usr/bin/t {names}\n\
                    twoout /usr/bin/t {out} {out} {names}\n\
                    nosel /usr/bin/t {out}\n\
                    glued /usr/bin/t {out} x{names}\n\
                    .dot /usr/bin/t {out} {names}\n\
                    gone /nowhere/t {out} {names}\n\
                    ok /usr/bin/t {out} {names}\n\
                    ok /usr/bin/u {out} {names}\n";
        let resolve = |p: &Path| (p != Path::new("/nowhere/t")).then(|| p.to_path_buf());
        let (formats, refused) = parse(text, &["zip"], resolve);
        assert_eq!(formats.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["ok"]);
        assert_eq!(formats[0].program, Path::new("/usr/bin/t"), "the first line of an id wins");
        assert_eq!(refused.len(), 10, "{:#?}", refused);
        for (i, line) in refused.iter().enumerate() {
            let want = if i < 9 { i + 1 } else { 11 };
            assert!(line.starts_with(&format!("line {}: ", want)), "{}", line);
        }
        assert!(refused[1].contains("already a format"), "a built-in id is never replaced");
    }

    #[test]
    fn the_submenu_is_capped() {
        let text: String = (0..MAX_FORMATS + 2).map(|i| format!("f{} /usr/bin/t {{out}} {{names}}\n", i)).collect();
        let (formats, refused) = parse(&text, &[], found);
        assert_eq!(formats.len(), MAX_FORMATS);
        assert_eq!(refused.len(), 2);
    }

    #[test]
    fn ids_are_extensions() {
        for ok in ["zjx", "tar.lz4", "a1", "x.y.z"] {
            assert!(valid_id(ok), "{}", ok);
        }
        for bad in ["", "ZJX", ".zjx", "zjx.", "a..b", "a/b", "a b", "z-j", "abcdefghijklmnopq"] {
            assert!(!valid_id(bad), "{}", bad);
        }
    }

    #[test]
    fn only_an_executable_regular_file_is_a_program() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("flea-archiveuser-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(executable(&tool).is_none(), "no execute bit");
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&tool, &link).unwrap();
        assert_eq!(executable(&link), Some(std::fs::canonicalize(&tool).unwrap()), "a link binds its target");
        assert!(executable(&dir).is_none(), "a directory is not a program");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
