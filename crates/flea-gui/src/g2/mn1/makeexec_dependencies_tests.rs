use super::{backend_extern_args, locked_dependencies};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const LOCK: &str = r#"
[[package]]
name = "flea"
version = "0.4.0"
dependencies = [
 "foldhash 0.2.0",
 "regex",
 "regex-syntax 0.8.11",
 "unicode-normalization",
]

[[package]]
name = "foldhash"
version = "0.1.5"

[[package]]
name = "regex"
version = "1.13.1"
dependencies = [
 "regex-automata",
]

[[package]]
name = "regex-syntax"
version = "0.8.10"

[[package]]
name = "regex-automata"
version = "0.4.14"

[[package]]
name = "unicode-normalization"
version = "0.1.25"
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let root = std::env::temp_dir().join(format!(
            "flea-backend-dependencies-{}-{}",
            std::process::id(),
            stamp.as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
        Self(root)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn backend_externs_cover_direct_dependencies_with_locked_versions_and_crate_names() {
    let root = Scratch::new();
    std::fs::write(root.0.join("Cargo.lock"), LOCK).unwrap();
    let dependencies = root.0.join("deps");
    std::fs::create_dir(&dependencies).unwrap();
    let packages = [
        ("foldhash", "0.2.0"),
        ("regex", "1.13.1"),
        ("regex-syntax", "0.8.11"),
        ("unicode-normalization", "0.1.25"),
    ];
    assert_eq!(locked_dependencies(LOCK), packages);
    let mut expected = Vec::new();
    for (package, version) in packages {
        let name = package.replace('-', "_");
        let library = dependencies.join(format!("lib{name}-built.rlib"));
        std::fs::write(&library, "").unwrap();
        std::fs::write(
            dependencies.join(format!("{name}-built.d")),
            format!("target: /registry/{package}-{version}/src/lib.rs\n"),
        )
        .unwrap();
        // A newer check-only artifact and an old version must not replace the built rlib.
        std::fs::write(
            dependencies.join(format!("{name}-check.d")),
            format!("target: /registry/{package}-{version}/src/lib.rs\n"),
        )
        .unwrap();
        std::fs::write(dependencies.join(format!("lib{name}-stale.rlib")), "").unwrap();
        std::fs::write(
            dependencies.join(format!("{name}-stale.d")),
            format!("target: /registry/{package}-0.0.0/src/lib.rs\n"),
        )
        .unwrap();
        expected.extend([
            "--extern".to_string(),
            format!("{name}={}", library.display()),
        ]);
    }
    assert_eq!(backend_extern_args(&root.0, &dependencies), expected);
}
