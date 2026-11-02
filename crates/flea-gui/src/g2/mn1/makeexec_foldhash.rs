//! Locked direct dependencies for the fixtures that build the backend with bare rustc.
use std::path::{Path, PathBuf};

fn named(block: &str, name: &str) -> bool {
    block
        .lines()
        .any(|line| line == format!("name = \"{name}\""))
}

// Sample input, the flea package block of Cargo.lock; a lone resolved version is written bare as "foldhash":
// [[package]]
// name = "flea"
// version = "0.4.0"
// dependencies = [
//  "foldhash 0.2.0",
// ]
fn locked_dependencies(lock: &str) -> Vec<(&str, &str)> {
    let packages: Vec<&str> = lock.split("[[package]]").collect();
    let flea = packages
        .iter()
        .find(|block| named(block, "flea"))
        .expect("Cargo.lock holds the flea package");
    let (_, dependencies) = flea
        .split_once("dependencies = [")
        .expect("the flea package has direct dependencies");
    let (dependencies, _) = dependencies.split_once(']').unwrap();
    dependencies
        .lines()
        .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let mut words = entry.split_whitespace();
            let name = words.next().unwrap();
            let version = words.next().unwrap_or_else(|| {
                packages
                    .iter()
                    .find(|block| named(block, name))
                    .expect("Cargo.lock holds the dependency package")
                    .lines()
                    .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
                    .expect("the dependency package carries a version")
            });
            (name, version)
        })
        .collect()
}

// Sample input (deps/foldhash-<hash>.d): /x-target/debug/deps/foldhash-1a2b3c.d: /home/gm/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/foldhash-0.2.0/src/lib.rs
// Takes the newest rlib for the locked version, the latest build's; a dep-info file with no rlib (a cargo check) is skipped.
fn extern_arg(dependencies: &Path, package: &str, version: &str) -> String {
    let name = package.replace('-', "_");
    let source = format!("/{package}-{version}/");
    let library: PathBuf = std::fs::read_dir(dependencies)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let file = path.file_name().unwrap().to_string_lossy();
            file.starts_with(&format!("{name}-"))
                && file.ends_with(".d")
                && std::fs::read_to_string(path).is_ok_and(|text| text.contains(&source))
        })
        .map(|info| {
            let stem = info.file_stem().unwrap().to_string_lossy().into_owned();
            dependencies.join(format!("lib{stem}.rlib"))
        })
        .filter_map(|library| Some((library.metadata().ok()?.modified().ok()?, library)))
        .max()
        .map(|(_, library)| library)
        .unwrap_or_else(|| panic!("cargo built the backend's {package} {version} for this test"));
    format!("{name}={}", library.display())
}

// Sample input, a test unit's fingerprint json, cut to its flea entry ([unit id, crate, public, fingerprint]):
// {"rustc":1625334936438085654,"deps":[[8834117861150897078,"flea",false,7573756887539753601]],"local":[]}
fn flea_fingerprint(unit: &str) -> Option<u64> {
    let (_, entry) = unit.split_once(",\"flea\",")?;
    let (_, value) = entry.split_once(',')?;
    value[..value.find(']')?].parse().ok()
}

// A shared target dir holds several libflea rlibs, so the one this test links is the one its own cargo fingerprint names.
pub(crate) fn core_library(dependencies: &Path, test_binary: &Path) -> PathBuf {
    let fingerprints = dependencies
        .parent()
        .unwrap_or_else(|| panic!("deps dir {} has no target dir", dependencies.display()))
        .join(".fingerprint");
    let name = test_binary.file_name().unwrap().to_string_lossy();
    let (_, hash) = name
        .rsplit_once('-')
        .unwrap_or_else(|| panic!("test binary {name} carries no cargo hash"));
    let unit_dir = fingerprints.join(format!("flea-gui-{hash}"));
    let unit = std::fs::read_dir(&unit_dir)
        .unwrap_or_else(|error| {
            panic!(
                "cannot read test fingerprint {}: {error}",
                unit_dir.display()
            )
        })
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            name.starts_with("test-") && name.ends_with(".json")
        })
        .unwrap_or_else(|| panic!("cargo wrote no test fingerprint in {}", unit_dir.display()));
    let recorded = std::fs::read_to_string(&unit)
        .unwrap_or_else(|error| panic!("cannot read test fingerprint {}: {error}", unit.display()));
    let linked = flea_fingerprint(&recorded).unwrap_or_else(|| {
        panic!(
            "test fingerprint {} lists no flea dependency",
            unit.display()
        )
    });
    // cargo writes lib-flea as the fingerprint's little-endian bytes in hex.
    let wanted = format!("{:016x}", linked.swap_bytes());
    std::fs::read_dir(&fingerprints)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", fingerprints.display()))
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let library_hash = name.strip_prefix("flea-")?.to_owned();
            let recorded = std::fs::read_to_string(entry.path().join("lib-flea")).ok()?;
            (recorded.trim() == wanted).then_some(library_hash)
        })
        .map(|library_hash| dependencies.join(format!("libflea-{library_hash}.rlib")))
        .find(|library| library.is_file())
        .unwrap_or_else(|| {
            panic!(
                "no flea rlib in {} carries fingerprint {wanted}, which {} links",
                dependencies.display(),
                unit.display()
            )
        })
}

// -L resolves transitive crates, but each direct dependency also needs --extern.
pub(crate) fn backend_extern_args(workspace: &Path, dependencies: &Path) -> Vec<String> {
    let lock = std::fs::read_to_string(workspace.join("Cargo.lock")).unwrap();
    locked_dependencies(&lock)
        .into_iter()
        .flat_map(|(package, version)| {
            [
                "--extern".to_string(),
                extern_arg(dependencies, package, version),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::locked_dependencies;

    #[test]
    fn makeexec_foldhash_reads_the_locked_version_named_or_bare() {
        let named = "[[package]]\nname = \"flea\"\nversion = \"0.4.0\"\ndependencies = [\n \"foldhash 0.2.0\",\n]\n\n[[package]]\nname = \"foldhash\"\nversion = \"0.1.5\"\n";
        assert_eq!(locked_dependencies(named), vec![("foldhash", "0.2.0")]);
        let bare = "[[package]]\nname = \"flea\"\nversion = \"0.4.0\"\ndependencies = [\n \"foldhash\",\n]\n\n[[package]]\nname = \"foldhash\"\nversion = \"0.2.1\"\n";
        assert_eq!(locked_dependencies(bare), vec![("foldhash", "0.2.1")]);
    }
}

#[cfg(test)]
#[path = "makeexec_dependencies_tests.rs"]
mod dependencies_tests;
