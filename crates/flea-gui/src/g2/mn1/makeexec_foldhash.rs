//! The foldhash rlib for the fixtures that build the backend with bare rustc.
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
fn locked_version(lock: &str) -> String {
    let packages: Vec<&str> = lock.split("[[package]]").collect();
    let flea = packages
        .iter()
        .find(|block| named(block, "flea"))
        .expect("Cargo.lock holds the flea package");
    let entry = flea
        .lines()
        .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
        .find(|entry| *entry == "foldhash" || entry.starts_with("foldhash "))
        .expect("the flea package depends on foldhash");
    if let Some(version) = entry.strip_prefix("foldhash ") {
        return version.to_string();
    }
    let block = packages
        .iter()
        .find(|block| named(block, "foldhash"))
        .expect("Cargo.lock holds the foldhash package");
    block
        .lines()
        .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
        .expect("the foldhash package carries a version")
        .to_string()
}

// Sample input (deps/foldhash-<hash>.d): /x-target/debug/deps/foldhash-1a2b3c.d: /home/gm/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/foldhash-0.2.0/src/lib.rs
// Takes the newest rlib for the locked version, the latest build's; a dep-info file with no rlib (a cargo check) is skipped.
pub(crate) fn extern_arg(workspace: &Path, dependencies: &Path) -> String {
    let lock = std::fs::read_to_string(workspace.join("Cargo.lock")).unwrap();
    let source = format!("/foldhash-{}/", locked_version(&lock));
    let library: PathBuf = std::fs::read_dir(dependencies)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            name.starts_with("foldhash-")
                && name.ends_with(".d")
                && std::fs::read_to_string(path).is_ok_and(|text| text.contains(&source))
        })
        .map(|info| {
            let stem = info.file_stem().unwrap().to_string_lossy().into_owned();
            dependencies.join(format!("lib{stem}.rlib"))
        })
        .filter_map(|library| Some((library.metadata().ok()?.modified().ok()?, library)))
        .max()
        .map(|(_, library)| library)
        .expect("cargo built the backend's foldhash for this test");
    format!("foldhash={}", library.display())
}

#[cfg(test)]
mod tests {
    use super::locked_version;

    #[test]
    fn makeexec_foldhash_reads_the_locked_version_named_or_bare() {
        let named = "[[package]]\nname = \"flea\"\nversion = \"0.4.0\"\ndependencies = [\n \"foldhash 0.2.0\",\n]\n\n[[package]]\nname = \"foldhash\"\nversion = \"0.1.5\"\n";
        assert_eq!(locked_version(named), "0.2.0");
        let bare = "[[package]]\nname = \"flea\"\nversion = \"0.4.0\"\ndependencies = [\n \"foldhash\",\n]\n\n[[package]]\nname = \"foldhash\"\nversion = \"0.2.1\"\n";
        assert_eq!(locked_version(bare), "0.2.1");
    }
}
