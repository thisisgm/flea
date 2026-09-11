use crate::backend::thumbcache::uri_for;
use crate::backend::thumbspec::Spec;
use std::path::{Path, PathBuf};

// argv returns the canonicalised input beside the argv, so a caller cannot bind one path and hand the child another; see AGENTS.md "Thumbnailer specs".
pub fn argv(spec: &Spec, input: &Path, output: &Path, size: u32) -> Option<(PathBuf, Vec<String>)> {
    let abs = std::fs::canonicalize(input).ok()?;
    // %i and %u are two spellings of the one input the sandbox bound, so the URI is built here and never handed in; see AGENTS.md "Thumbnail cache".
    let child_uri = uri_for(&abs);
    let mut out = Vec::with_capacity(spec.exec.len());
    for tok in spec.exec.iter() {
        // corner: only a whole-token placeholder substitutes, which is every one the shipped field uses; see AGENTS.md "Thumbnailer specs".
        let t = match tok.as_str() {
            "%i" => abs.to_string_lossy().to_string(),
            "%u" => child_uri.clone(),
            "%o" => output.to_string_lossy().to_string(),
            "%s" => size.to_string(),
            other => other.to_string(),
        };
        out.push(t);
    }
    Some((abs, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::aliases::Aliases;
    use crate::backend::testdir::TestDir;
    use crate::backend::thumbspec::Thumbnailers;

    // A table of exactly one spec, so a test names the Exec line it is substituting and nothing else.
    fn spec_for(exec: &str) -> Thumbnailers {
        let aliases = Aliases::from_str("image/heic image/heif\n");
        let body = format!("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec={}\nMimeType=image/jpeg;\n", exec);
        Thumbnailers::from_entries(&[("t.thumbnailer".to_string(), body)], &aliases)
    }

    #[test]
    fn every_placeholder_substitutes_and_a_hostile_name_stays_one_argument() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh -i %i -u %u -o %o -s %s");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        // The file has to exist, or this would prove the refusal path and say nothing about substitution.
        let dir = TestDir::new("thumbargv-hostile");
        let hostile = dir.join("a; rm -rf b.jpg");
        std::fs::write(&hostile, b"").unwrap();
        let got = argv(s, &hostile, Path::new("/tmp/out.png"), 256);
        let (_abs, got) = got.unwrap();
        assert_eq!(got[0], "/bin/sh");
        assert!(got[2].starts_with('/'));
        assert!(got[2].ends_with("a; rm -rf b.jpg"));
        assert!(got[4].starts_with("file:///"));
        assert!(got[4].ends_with("a%3B%20rm%20-rf%20b.jpg"));
        assert_eq!(got[6], "/tmp/out.png");
        assert_eq!(got[8], "256");
        assert_eq!(got.len(), 9);
    }

    #[test]
    fn the_child_uri_names_the_canonical_path_and_not_the_one_that_was_asked_for() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh %u %o");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let dir = TestDir::new("thumbargv-uri");
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::write(dir.join("real/pic.jpg"), b"").unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
        let got = argv(s, &dir.join("link/pic.jpg"), Path::new("/tmp/o.png"), 256);
        let (abs, got) = got.unwrap();
        // The sandbox binds abs, so a %u naming the link sends the child at a path that does not exist inside the namespace.
        assert!(got[1].contains("/real/pic.jpg"), "%u kept the path that was asked for: {}", got[1]);
        assert!(!got[1].contains("/link/"));
        assert_eq!(got[1], uri_for(&abs));
    }

    #[test]
    fn an_input_that_cannot_be_resolved_is_refused_rather_than_substituted_raw() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh %i %o");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let gone = Path::new("/definitely/not/here.jpg");
        assert!(argv(s, gone, Path::new("/tmp/o.png"), 256).is_none());
    }

    #[test]
    fn a_relative_input_is_made_absolute_so_it_cannot_be_read_as_a_flag() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh %i %o");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        // /etc exists, so canonicalize succeeds and the result must be absolute.
        let (abs, got) = argv(s, Path::new("/etc/../etc"), Path::new("/tmp/o.png"), 256).unwrap();
        assert!(got[1].starts_with('/'));
        assert!(!got[1].contains(".."));
        assert_eq!(abs.to_string_lossy(), got[1]);
    }

    #[test]
    fn the_returned_input_is_the_one_the_child_is_told_to_open() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh %i %o");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let dir = TestDir::new("thumbargv-link");
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::write(dir.join("real/clip.jpg"), b"").unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
        let got = argv(s, &dir.join("link/clip.jpg"), Path::new("/tmp/o.png"), 256);
        let (abs, got) = got.unwrap();
        // The sandbox binds this path, so it must be the same string the child is handed, not the one the caller passed in.
        assert_eq!(abs.to_string_lossy(), got[1]);
        assert!(got[1].contains("/real/clip.jpg"));
        assert!(!got[1].contains("/link/"));
    }
}
