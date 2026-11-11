//! Compile the production plugin host for the tests that drive the real owner process.
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use crate::g2::context_menus::foldhash;

// Each test process owns its cached executable so concurrent suites cannot replace it.
pub(super) fn binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = std::env::current_exe().unwrap();
        let dependencies = executable.parent().unwrap();
        let library = foldhash::core_library(dependencies, &executable);
        let binary = dependencies.join(format!("flea-plugin-host-{}", std::process::id()));
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
            "production plugin host compile: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        binary
    })
}

#[path = "menu_backend_process_tests.rs"]
mod process_tests;
