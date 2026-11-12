//! The real filter producer, isolated in a marked temporary home and a 5000-file folder.
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

pub const TOTAL: usize = 5000;

pub fn name(row: usize) -> String {
    format!("file-{:04}.txt", row + 1)
}

pub struct Fixture(PathBuf);

impl Fixture {
    pub fn new() -> Self {
        let output = Command::new("mktemp")
            .args(["-d", "-t", "flea-gh315-XXXXXX"])
            .output()
            .expect("mktemp");
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        std::fs::write(
            root.join(".flea-test-sandbox"),
            "K315 backend protocol fixture\n",
        )
        .unwrap();
        std::fs::create_dir(root.join("tree")).unwrap();
        for row in 0..TOTAL {
            std::fs::File::create_new(root.join("tree").join(name(row))).unwrap();
        }
        Self(root)
    }

    pub fn answer(&self, query: &str, id: usize) -> String {
        // The normal workspace build supplies this sibling, as tests/gui-ipc.sh also requires.
        let binary = std::env::var_os("FLEA_GH315_BACKEND")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::cargo_artifacts::binary("flea"));
        assert!(
            binary.is_file(),
            "build the matching flea backend before these tests: {}",
            binary.display()
        );
        let mut child = Command::new("timeout")
            .args(["30"])
            .arg(binary)
            .arg("--backend")
            .env("HOME", &self.0)
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .env("XDG_CACHE_HOME", self.0.join("cache"))
            .env("XDG_STATE_HOME", self.0.join("state"))
            .env("XDG_DATA_HOME", self.0.join("data"))
            .env("XDG_RUNTIME_DIR", &self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("real backend");
        let path = flea::json::escape(self.0.join("tree").to_str().unwrap());
        let query = flea::json::escape(query);
        writeln!(child.stdin.take().unwrap(),
            "{{\"c\":\"list\",\"path\":\"{path}\",\"first\":320}}\n{{\"c\":\"filter\",\"query\":\"{query}\",\"id\":{id}}}\n{{\"c\":\"quit\"}}"
        ).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = String::from_utf8(output.stdout).unwrap();
        let answers: Vec<_> = output
            .lines()
            .filter(|line| flea::json::field_str(line, "t").as_deref() == Some("filtered"))
            .collect();
        assert_eq!(answers.len(), 1, "one filter reply: {output}");
        assert_eq!(flea::json::field_usize(answers[0], "n"), Some(TOTAL));
        assert_eq!(flea::json::field_usize(answers[0], "listing"), Some(1));
        answers[0].to_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("flea-gh315-")
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).expect("remove only the owned marked fixture");
    }
}
