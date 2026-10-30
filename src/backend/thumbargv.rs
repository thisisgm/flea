use crate::backend::thumbcache::uri_for;
use crate::backend::thumbspec::Spec;
use std::path::{Path, PathBuf};

// argv returns the canonicalised input beside the argv, so a caller cannot bind one path and hand the child another; see AGENTS.md "Thumbnailer specs".
pub fn argv(spec: &Spec, input: &Path, output: &Path, size: u32) -> Option<(PathBuf, Vec<String>)> {
    let abs = std::fs::canonicalize(input).ok()?;
    // %i and %u are two spellings of the one input the sandbox bound, so the URI is built here and never handed in; see AGENTS.md "Thumbnail cache".
    let child_uri = uri_for(&abs);
    let input = abs.to_string_lossy();
    let output = output.to_string_lossy();
    let size = size.to_string();
    let mut out = Vec::with_capacity(spec.exec.len());
    for (index, tok) in spec.exec.iter().enumerate() {
        // The validated program is literal; only its arguments contain field codes.
        out.push(if index == 0 { tok.clone() } else { expand(tok, &input, &child_uri, &output, &size) });
    }
    Some((abs, out))
}

// Scan once so percent signs inside a replacement remain filename bytes, never another field code.
fn expand(token: &str, input: &str, uri: &str, output: &str, size: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('i') => out.push_str(input),
            Some('u') => out.push_str(uri),
            Some('o') => out.push_str(output),
            Some('s') => out.push_str(size),
            Some('%') => out.push('%'),
            Some(other) => { out.push('%'); out.push(other); }
            None => out.push('%'),
        }
    }
    out
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

    #[test]
    fn f3d_options_expand_embedded_and_repeated_placeholders() {
        let a = Aliases::from_str("");
        // F3D's packaged Exec uses --output=%o and --resolution=%s,%s.
        let t = spec_for("/bin/sh --output=%o --resolution=%s,%s %i --uri=%u");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let dir = TestDir::new("thumbargv-f3d");
        let input = dir.file("white model.stl", "solid model\nendsolid model\n");
        let output = dir.join("rendered model.png");
        let (abs, got) = argv(s, &input, &output, 256).unwrap();
        assert_eq!(got, vec![
            "/bin/sh".to_string(), format!("--output={}", output.display()),
            "--resolution=256,256".to_string(), abs.to_string_lossy().to_string(),
            format!("--uri={}", uri_for(&abs)),
        ]);
    }

    #[test]
    fn replacements_are_literal_and_are_never_expanded_again() {
        let a = Aliases::from_str("");
        let dir = TestDir::new("thumbargv-literal");
        let program = dir.script("record-args", "#!/bin/sh\nprintf '%s\\0' \"$@\"\n");
        let t = spec_for(&format!("{} --input=%i --output=%o --uri=%u %%i %x tail% %%%%s", program.display()));
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let input = dir.file("雪 %o $(touch canary); 'quote' \"double\".stl", "");
        let output = dir.join("output %s.png");
        let (abs, got) = argv(s, &input, &output, 256).unwrap();
        assert_eq!(got[1], format!("--input={}", abs.display()));
        assert_eq!(got[2], format!("--output={}", output.display()));
        assert_eq!(got[3], format!("--uri={}", uri_for(&abs)));
        assert_eq!(&got[4..], &["%i", "%x", "tail%", "%%s"]);
        let child = std::process::Command::new(&got[0]).args(&got[1..]).current_dir(dir.path()).output().unwrap();
        assert!(child.status.success());
        assert_eq!(child.stdout, format!("{}\0", got[1..].join("\0")).as_bytes());
        assert!(!dir.join("canary").exists());
    }

    #[test]
    fn an_embedded_input_and_uri_still_name_the_sandbox_bind() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh --input=%i --uri=%u");
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let dir = TestDir::new("thumbargv-embedded-link");
        let input = dir.file("real.stl", "");
        std::os::unix::fs::symlink(&input, dir.join("link.stl")).unwrap();
        let (abs, got) = argv(s, &dir.join("link.stl"), &dir.join("out.png"), 256).unwrap();
        assert_eq!(abs, input.canonicalize().unwrap());
        assert_eq!(got[1], format!("--input={}", abs.display()));
        assert_eq!(got[2], format!("--uri={}", uri_for(&abs)));
    }

    #[test]
    fn the_validated_program_is_not_a_placeholder_template() {
        let dir = TestDir::new("thumbargv-program");
        let program = dir.script("decoder-%s", "#!/bin/sh\nexit 0\n");
        let a = Aliases::from_str("");
        let t = spec_for(&format!("{} --input=%i", program.display()));
        let s = t.for_mime("image/jpeg", &a).unwrap();
        let input = dir.file("model.stl", "");
        let (_, got) = argv(s, &input, &dir.join("out.png"), 256).unwrap();
        assert_eq!(got[0], program.to_string_lossy());
    }

    #[test]
    fn failures_from_whole_token_expansion_are_retried() {
        let a = Aliases::from_str("");
        let t = spec_for("/bin/sh --output=%o --resolution=%s,%s %i");
        let spec = t.for_mime("image/jpeg", &a).unwrap();
        let old_table = format!("image/jpeg\0{}", spec.exec.join("\0"));
        let old_fingerprint = format!("{:016x}", crate::backend::xxh64::hash(old_table.as_bytes()));
        assert_ne!(t.fingerprint(), old_fingerprint);
    }
}
