//! The workspace backend supplies real probe receipts and journaled permission writes to the mounted tests.
use flea::jsondoc::{self, Json};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Receiver},
        OnceLock,
    },
    time::Duration,
};

use crate::g2::mn1::foldhash;

pub struct Process {
    child: Child,
    lines: Receiver<String>,
}

fn binary(root: &Path) -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = std::env::current_exe().unwrap();
        let dependencies = executable.parent().unwrap();
        let library = foldhash::core_library(dependencies, &executable);
        let binary = root.join("flea-backend");
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg("--crate-name=flea")
            .arg(workspace.join("src/main.rs"))
            .arg("--extern")
            .arg(format!("flea={}", library.display()))
            .args(foldhash::backend_extern_args(&workspace, dependencies))
            .arg("-L")
            .arg(format!("dependency={}", dependencies.display()))
            .arg("-o")
            .arg(&binary)
            .env("CARGO_MANIFEST_DIR", &workspace)
            .env("CARGO_PKG_VERSION", env!("CARGO_PKG_VERSION"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "the real backend fixture must compile: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        binary
    })
}

impl Process {
    pub fn new(root: &Path) -> Self {
        assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
        let mut child = Command::new(binary(root))
            .arg("--backend")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_STATE_HOME", root.join("state"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    pub fn request(&mut self, request: &Json, kind: &str) -> String {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", flea::ui_logic_st2::wire::compact(request)).unwrap();
        stdin.flush().unwrap();
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(5))
                .expect("the real backend must answer the fixture request");
            let reply = jsondoc::parse(&line).unwrap();
            if reply.get("t").and_then(Json::as_str) == Some(kind) {
                return line;
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().unwrap();
    }
}
