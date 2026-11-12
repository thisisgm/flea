//! Locked direct dependencies for the fixtures that build the backend with bare rustc.
use std::path::{Path, PathBuf};

// Sample input: <profile>/.fingerprint/flea-1a2b with rlibs in <profile>/deps (Cargo 1.99), or <profile>/build/flea/1a2b/{fingerprint,out} (Cargo 1.100).
fn unit_dirs(profile: &Path, package: &str) -> Vec<(PathBuf, PathBuf)> {
    let mut units = Vec::new();
    if let Ok(entries) = std::fs::read_dir(profile.join(".fingerprint")) {
        for entry in entries.filter_map(Result::ok) {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{package}-"))
            {
                units.push((entry.path(), profile.join("deps")));
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(profile.join("build").join(package)) {
        for entry in entries.filter_map(Result::ok) {
            units.push((entry.path().join("fingerprint"), entry.path().join("out")));
        }
    }
    units
}

// Sample input: <profile>/deps/<test> (Cargo 1.99) gives <profile>/deps; <profile>/build/flea-gui/<hash>/out/<test> (Cargo 1.100) gives every <profile>/build/<package>/<hash>/out, sorted.
// A target dir shared by both toolchains holds both layouts, and only the test's own layout was built by its rustc.
fn dependency_dirs(test_binary: &Path) -> Vec<PathBuf> {
    let profile = crate::cargo_artifacts::profile_dir(test_binary);
    let modern = test_binary
        .parent()
        .is_some_and(|directory| directory.ends_with("out"));
    if !modern {
        return vec![profile.join("deps")];
    }
    let mut directories = Vec::new();
    if let Ok(packages) = std::fs::read_dir(profile.join("build")) {
        for package in packages.filter_map(Result::ok) {
            if let Ok(units) = std::fs::read_dir(package.path()) {
                directories.extend(
                    units
                        .filter_map(Result::ok)
                        .map(|unit| unit.path().join("out"))
                        .filter(|out| out.is_dir()),
                );
            }
        }
    }
    directories.sort();
    directories
}

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

// Sample input (deps/foldhash-<hash>.d, or build/foldhash/<hash>/out/foldhash-<hash>.d on Cargo 1.100): /x-target/debug/deps/foldhash-1a2b3c.d: /home/gm/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/foldhash-0.2.0/src/lib.rs
// Takes the newest rlib for the locked version, the latest build's; a dep-info file with no rlib (a cargo check) is skipped.
fn extern_arg(dependencies: &[PathBuf], package: &str, version: &str) -> String {
    let name = package.replace('-', "_");
    let source = format!("/{package}-{version}/");
    let library: PathBuf = dependencies
        .iter()
        .flat_map(|directory| std::fs::read_dir(directory).unwrap())
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
            info.parent().unwrap().join(format!("lib{stem}.rlib"))
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
pub(crate) fn core_library(test_binary: &Path) -> PathBuf {
    let profile = crate::cargo_artifacts::profile_dir(test_binary);
    let name = test_binary.file_name().unwrap().to_string_lossy();
    let (_, hash) = name
        .rsplit_once('-')
        .unwrap_or_else(|| panic!("test binary {name} carries no cargo hash"));
    let unit_dir = if test_binary
        .parent()
        .unwrap()
        .file_name()
        .is_some_and(|name| name == "out")
    {
        test_binary
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fingerprint")
    } else {
        profile
            .join(".fingerprint")
            .join(format!("flea-gui-{hash}"))
    };
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
    unit_dirs(&profile, "flea")
        .into_iter()
        .filter_map(|(fingerprint, out)| {
            let recorded = std::fs::read_to_string(fingerprint.join("lib-flea")).ok()?;
            if recorded.trim() != wanted {
                return None;
            }
            let name = fingerprint.file_name()?.to_string_lossy();
            let library_hash = if name == "fingerprint" {
                fingerprint
                    .parent()?
                    .file_name()?
                    .to_string_lossy()
                    .into_owned()
            } else {
                name.strip_prefix("flea-")?.to_owned()
            };
            Some(out.join(format!("libflea-{library_hash}.rlib")))
        })
        .find(|library| library.is_file())
        .unwrap_or_else(|| {
            panic!(
                "no flea rlib in {} carries fingerprint {wanted}, which {} links",
                profile.display(),
                unit.display()
            )
        })
}

// -L resolves transitive crates, but each direct dependency also needs --extern.
pub(crate) fn backend_extern_args(workspace: &Path, test_binary: &Path) -> Vec<String> {
    let dependencies = dependency_dirs(test_binary);
    let lock = std::fs::read_to_string(workspace.join("Cargo.lock")).unwrap();
    locked_dependencies(&lock)
        .into_iter()
        .flat_map(|(package, version)| {
            [
                "--extern".to_string(),
                extern_arg(&dependencies, package, version),
            ]
        })
        .chain(dependencies.iter().flat_map(|directory| {
            [
                "-L".to_owned(),
                format!("dependency={}", directory.display()),
            ]
        }))
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
