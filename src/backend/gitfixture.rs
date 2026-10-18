// Every mutating Git fixture lives in its own mktemp root, with containment checked before writes and removal.
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub struct Fixture(pub PathBuf);

impl Fixture {
    pub fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        assert!(parent.is_absolute() && parent != Path::new("/"));
        let home = PathBuf::from(std::env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap();
        assert!(
            !parent.starts_with(home),
            "Git fixture temporary root must be outside HOME"
        );
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("flea-git-test.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim_end());
        assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
        std::fs::write(path.join(".flea-test-sandbox"), "git fixture\n").unwrap();
        Self(path)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        self.check(&path);
        path
    }

    pub fn check(&self, path: &Path) {
        assert!(self.0.is_absolute() && self.0.join(".flea-test-sandbox").is_file());
        assert!(path.is_absolute() && !path.as_os_str().is_empty() && path.starts_with(&self.0));
        assert!(!path.components().any(|part| part == Component::ParentDir));
    }

    pub fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    pub fn git(&self, args: &[&str]) {
        self.check(&self.0);
        let mut command = Command::new("/usr/bin/git");
        command
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .env("XDG_CONFIG_HOME", &self.0)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=GM",
                "-c",
                "user.email=gianmarcomorales@icloud.com",
            ])
            .args(args);
        if args.first() == Some(&"commit") {
            command.arg("--author=GM <gianmarcomorales@icloud.com>");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    pub fn repo(&self) {
        self.git(&["init", "--initial-branch=main", "--quiet"]);
        self.write(".gitignore", b".flea-test-sandbox\n");
    }

    pub fn available() -> bool {
        if Command::new("/usr/bin/git")
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
        {
            return true;
        }
        use std::io::Write;
        let _ = std::io::stderr().write_all(b"SKIP git fixture: /usr/bin/git is unavailable\n");
        false
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.check(&self.0);
        assert!(self.0.parent() == Some(std::env::temp_dir().canonicalize().unwrap().as_path()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("flea-git-test."));
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
