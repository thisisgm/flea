use super::{backend_extern_args, core_library, locked_dependencies};
use std::{
    path::{Path, PathBuf},
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

// A libflea rlib and the lib-flea fingerprint cargo writes beside it, as little-endian bytes in hex.
fn built_flea(target: &Path, hash: &str, fingerprint: u64) {
    std::fs::write(target.join(format!("deps/libflea-{hash}.rlib")), "").unwrap();
    let unit = target.join(format!(".fingerprint/flea-{hash}"));
    std::fs::create_dir(&unit).unwrap();
    std::fs::write(
        unit.join("lib-flea"),
        format!("{:016x}", fingerprint.swap_bytes()),
    )
    .unwrap();
}

// A flea-gui test binary whose own fingerprint names the flea fingerprint it links.
fn linked_test(target: &Path, hash: &str, fingerprint: u64) -> PathBuf {
    let unit = target.join(format!(".fingerprint/flea-gui-{hash}"));
    std::fs::create_dir(&unit).unwrap();
    std::fs::write(
        unit.join("test-lib-flea_gui.json"),
        format!(r#"{{"rustc":1,"deps":[[2,"flea_gui",false,3],[4,"flea",false,{fingerprint}]],"local":[]}}"#),
    )
    .unwrap();
    let test = target.join(format!("deps/flea_gui-{hash}"));
    std::fs::write(&test, "").unwrap();
    test
}

fn shared_target() -> Scratch {
    let root = Scratch::new();
    std::fs::create_dir(root.0.join("deps")).unwrap();
    std::fs::create_dir(root.0.join(".fingerprint")).unwrap();
    root
}

// EY-569: a stale rlib, and another tree's written after this test's rlib and before the test relinks, share the dir.
#[test]
fn fix_harness_gate_2_core_library_is_the_rlib_the_test_fingerprint_names() {
    let target = shared_target();
    built_flea(&target.0, "aaaaaaaaaaaaaaaa", 11);
    built_flea(&target.0, "bbbbbbbbbbbbbbbb", 7_573_756_887_539_753_601);
    built_flea(&target.0, "cccccccccccccccc", 33);
    let test = linked_test(&target.0, "eeeeeeeeeeeeeeee", 7_573_756_887_539_753_601);
    assert_eq!(
        core_library(&target.0.join("deps"), &test),
        target.0.join("deps/libflea-bbbbbbbbbbbbbbbb.rlib")
    );
}

#[test]
#[should_panic(expected = "carries fingerprint 81a2f900c3631b69")]
fn fix_harness_gate_2_no_rlib_with_the_linked_fingerprint_fails_loudly() {
    let target = shared_target();
    built_flea(&target.0, "aaaaaaaaaaaaaaaa", 11);
    let test = linked_test(&target.0, "eeeeeeeeeeeeeeee", 7_573_756_887_539_753_601);
    core_library(&target.0.join("deps"), &test);
}
