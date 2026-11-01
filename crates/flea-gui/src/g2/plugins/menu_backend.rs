//! Compile the production plugin host inside the cargo-west test sandbox, as the menu backend fixtures do.
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use crate::g2::mn1::foldhash;

pub(super) fn binary(root: &Path) -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = std::env::current_exe().unwrap();
        let dependencies = executable.parent().unwrap();
        let library = std::fs::read_dir(dependencies)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("libflea-")
                    && path.extension().is_some_and(|e| e == "rlib")
            })
            .max_by_key(|path| path.metadata().unwrap().modified().unwrap())
            .expect("cargo built flea");
        let binary = root.join("flea-plugin-host");
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg("--crate-name=flea")
            .arg(workspace.join("src/main.rs"))
            .arg("--extern")
            .arg(format!("flea={}", library.display()))
            .arg("--extern")
            .arg(foldhash::extern_arg(&workspace, dependencies))
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
