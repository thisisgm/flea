use super::*;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const MARKER: &str = ".flea-test-sandbox";

pub(crate) struct Fixture {
    root: PathBuf,
    parent: PathBuf,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/tmp");
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let output = Command::new("/usr/bin/mktemp")
            .arg("-d")
            .arg(parent.join("chafa-test.XXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim_end());
        assert!(root.is_absolute() && root.parent() == Some(parent.as_path()));
        File::options()
            .write(true)
            .create_new(true)
            .open(root.join(MARKER))
            .unwrap()
            .write_all(b"chafa test sandbox\n")
            .unwrap();
        let fixture = Self { root, parent };
        std::fs::create_dir(fixture.path("bin")).unwrap();
        fixture
    }

    fn owned(&self) -> bool {
        self.root.is_absolute()
            && !self.root.as_os_str().is_empty()
            && self.root.parent() == Some(self.parent.as_path())
            && self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("chafa-test.")
            && self
                .root
                .join(MARKER)
                .symlink_metadata()
                .is_ok_and(|m| m.is_file())
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        assert!(
            self.owned()
                && path.is_absolute()
                && !path.as_os_str().is_empty()
                && path.starts_with(&self.root)
                && !path
                    .components()
                    .any(|c| c == std::path::Component::ParentDir)
        );
        path
    }

    pub(crate) fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    pub(crate) fn stub(&self, body: &str) {
        let path = self.path("bin/chafa");
        // A child writes the script so concurrent fork/exec cannot inherit a writer and cause ETXTBSY.
        let mut writer = Command::new("/bin/sh")
            .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        writer
            .stdin
            .take()
            .unwrap()
            .write_all(format!("#!/bin/sh\nset -eu\n{body}\n").as_bytes())
            .unwrap();
        assert!(writer.wait().unwrap().success());
    }

    pub(crate) fn start(&self, columns: usize, rows: usize, colors: ColorMode) -> Job {
        let input =
            crate::backend::regfile::open_if_regular(&self.file("image", b"held\n"), 0).unwrap();
        Job::start_on(input, columns, rows, colors, self.path("bin").as_os_str())
            .unwrap()
            .unwrap()
    }

    pub(crate) fn output(&self, bytes: &[u8], columns: usize, rows: usize) -> io::Result<Grid> {
        let parsed = validate(bytes, columns, rows);
        if !sandboxed() {
            return parsed;
        }
        let octal: String = bytes.iter().map(|byte| format!("\\{byte:03o}")).collect();
        self.stub(&format!("printf '{octal}'"));
        let decoded = self
            .start(columns, rows, ColorMode::Truecolor)
            .result
            .recv_timeout(Duration::from_secs(15))
            .unwrap();
        assert_eq!(parsed.is_ok(), decoded.is_ok());
        if let (Ok(parsed), Ok(decoded)) = (&parsed, &decoded) {
            assert_eq!(parsed, decoded);
        }
        decoded
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.owned(),
            "refusing to remove an unmarked or non-absolute test root"
        );
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

pub(crate) fn sandboxed() -> bool {
    !crate::backend::sandboxprobe::skipped()
}

// Debian's ELF loader alias differs from Omarchy's; every namespace, mount permission and limit stays identical.
pub(crate) fn host_loader(argv: &mut [String]) {
    if !Path::new("/usr/lib/ld-linux-x86-64.so.2").exists()
        && Path::new("/usr/lib64/ld-linux-x86-64.so.2").exists()
    {
        for at in 0..argv.len().saturating_sub(2) {
            if argv[at..at + 3] == ["--symlink", "usr/lib", "/lib64"] {
                argv[at + 1] = "usr/lib64".into();
            }
        }
    }
}
