//! Parity with the recovered B3 fixture formula and digest, and the refusals that keep coverage honest.
use crate::adapters::{self, Mode};
use crate::{fixture, measure};
use std::path::{Path, PathBuf};

// Digests computed by the recovered fixtures.py and the fairness harness digest at the same sizes.
const PY_300: &str = "ff6ed5acfd951eb712f263080c67c07bf6b06ace4036ce81cd82eb28030644d4";
const PY_5000: &str = "8913cdc9178b829a88526e6d6996305c00fdea1a5a475da10cf7c1771383699d";

fn marked(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("flea-walkbench-{tag}-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join(fixture::SANDBOX_MARKER), "test\n").unwrap();
    root.canonicalize().unwrap()
}

fn full_digest(tree: &Path) -> String {
    fixture::digest(&mut fixture::census(tree).unwrap().items)
}

#[test]
fn digest_matches_the_fairness_harness() {
    let mut items = vec![
        (b"z.txt".to_vec(), b'f'),
        (b"a/b".to_vec(), b'f'),
        (b"a".to_vec(), b'd'),
    ];
    assert_eq!(
        fixture::digest(&mut items),
        "8db6f3fe5368d369f0bf9a551a295e13b8711acf8a0b37af38d9a447131d5f92"
    );
}

#[test]
fn synthetic_matches_fixtures_py() {
    let root = marked("parity");
    assert_eq!(
        full_digest(&fixture::synthetic(&root, 300).unwrap()),
        PY_300
    );
    assert_eq!(
        full_digest(&fixture::synthetic(&root, 5000).unwrap()),
        PY_5000
    );
}

#[test]
fn unmarked_relative_and_reused_roots_refuse() {
    let bare = std::env::temp_dir().join(format!("flea-walkbench-bare-{}", std::process::id()));
    std::fs::create_dir(&bare).unwrap();
    assert!(fixture::synthetic(&bare, 300).is_err());
    assert!(fixture::synthetic(Path::new("relative"), 300).is_err());
    let root = marked("reuse");
    fixture::synthetic(&root, 300).unwrap();
    assert!(
        fixture::synthetic(&root, 300).is_err(),
        "an existing fixture is never overwritten"
    );
}

#[test]
fn every_in_process_adapter_covers_the_census() {
    let root = marked("cover");
    let tree = fixture::synthetic(&root, 5000).unwrap();
    for name in [
        "std-serial",
        "ignore-parallel",
        "ignore-serial",
        "jwalk",
        "dua-core",
    ] {
        let outcome = adapters::run(name, &tree, 4, Mode::Index, None).unwrap();
        assert_eq!(
            (outcome.sink.files, outcome.sink.dirs, outcome.errors),
            (4829, 171, 0),
            "{name}"
        );
        assert_eq!(
            fixture::digest(&mut outcome.sink.items()),
            PY_5000,
            "{name}"
        );
        let walk = adapters::run(name, &tree, 4, Mode::Walk, None).unwrap();
        assert_eq!(
            (walk.sink.files, walk.sink.dirs),
            (4829, 171),
            "{name} walk"
        );
    }
}

fn trial(files: u64, dirs: u64, digest: &str) -> measure::Trial {
    measure::Trial {
        wall_ns: 1,
        stage_ns: 1,
        user_us: 0,
        sys_us: 0,
        maxrss_kib: 1,
        stage_cpu_us: 0,
        hwm_kib: 1,
        spawn_hwm_kib: 0,
        tool_maxrss_kib: 0,
        files,
        dirs,
        errors: 0,
        digest: digest.into(),
        exit: 0,
    }
}

#[test]
fn coverage_refuses_short_sets_and_wrong_identities() {
    assert_eq!(
        measure::coverage(&trial(10, 2, "full"), 10, 2, "full", "files", true, true),
        "ok"
    );
    assert_eq!(
        measure::coverage(&trial(9, 2, "full"), 10, 2, "full", "files", true, true),
        "count-mismatch"
    );
    assert_eq!(
        measure::coverage(&trial(10, 2, "other"), 10, 2, "full", "files", true, true),
        "identity-mismatch"
    );
    assert_eq!(
        measure::coverage(&trial(10, 0, "files"), 10, 2, "full", "files", false, true),
        "ok"
    );
    let mut failed = trial(10, 2, "full");
    failed.exit = 1;
    assert_eq!(
        measure::coverage(&failed, 10, 2, "full", "files", true, true),
        "error"
    );
}

#[test]
fn hidden_names_make_a_fixture_ineligible() {
    let root = marked("hidden");
    let tree = fixture::synthetic(&root, 300).unwrap();
    std::fs::write(tree.join(".hidden"), "").unwrap();
    assert_eq!(fixture::census(&tree).unwrap().hidden, 1);
    let filtered = adapters::run("jwalk", &tree, 2, Mode::Index, None).unwrap();
    assert_eq!(
        filtered.sink.files, 277,
        "unrestricted walkers see the hidden name the census excludes"
    );
}

#[test]
fn cold_without_a_controller_hook_refuses() {
    assert!(measure::cache_gate("cold", None).is_err());
    assert_eq!(
        measure::cache_gate("cold", Some("true")).unwrap(),
        "cold-hook-ok"
    );
    assert!(measure::cache_gate("cold", Some("false")).is_err());
    assert_eq!(measure::cache_gate("warm", None).unwrap(), "warm");
}

#[test]
fn stage_memory_and_cpu_are_self_reported() {
    let root = marked("selfreport");
    let tree = fixture::synthetic(&root, 300).unwrap();
    let outcome = adapters::run("std-serial", &tree, 1, Mode::Index, None).unwrap();
    assert!(outcome.hwm_kib > 0 && outcome.cpu_us >= 0);
    assert_eq!(
        (outcome.spawn_hwm_kib, outcome.tool_maxrss_kib),
        (0, 0),
        "no external tool ran"
    );
}
