//! Locate final Cargo artifacts without depending on an intermediate unit's hash.
use std::path::{Path, PathBuf};

// Sample input: <target>/<profile>/deps/flea_gui-1a2b (Cargo 1.99) or <target>/<profile>/build/flea-gui/1a2b/out/flea_gui-1a2b (Cargo 1.100), both give <target>/<profile>.
pub(crate) fn profile_dir(executable: &Path) -> PathBuf {
    for directory in executable.ancestors().skip(1) {
        if matches!(
            directory.file_name().and_then(|name| name.to_str()),
            Some("deps" | "examples")
        ) {
            return directory
                .parent()
                .expect("Cargo profile directory")
                .to_path_buf();
        }
        if directory.file_name().is_some_and(|name| name == "out") {
            if let Some(build) = directory.ancestors().nth(3) {
                if build.file_name().is_some_and(|name| name == "build") {
                    return build
                        .parent()
                        .expect("Cargo profile directory")
                        .to_path_buf();
                }
            }
        }
    }
    panic!(
        "cannot locate Cargo's final artifact directory from {}",
        executable.display()
    );
}

pub(crate) fn binary(name: &str) -> PathBuf {
    let declared = if name == "flea" {
        option_env!("CARGO_BIN_EXE_flea")
    } else {
        None
    };
    declared.map(PathBuf::from).unwrap_or_else(|| {
        profile_dir(&std::env::current_exe().expect("the test executable has a path")).join(name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_artifacts_share_the_profile_on_both_cargo_layouts() {
        for profile in ["debug", "release", "custom-profile"] {
            let root = PathBuf::from("/custom-target/x86_64-unknown-linux-gnu").join(profile);
            for relative in [
                "deps/flea_gui-1234",
                "build/flea-gui/1234/out/flea_gui-1234",
                "examples/ipc-fixture",
            ] {
                assert_eq!(profile_dir(&root.join(relative)), root);
            }
        }
    }

    #[test]
    fn a_missing_final_artifact_still_resolves_to_the_profile() {
        let root = Path::new("/not-built/release");
        assert_eq!(
            profile_dir(&root.join("build/flea/abcd/out/test")).join("flea"),
            root.join("flea")
        );
    }

    #[test]
    fn backend_path_keeps_the_final_binary_name() {
        assert_eq!(binary("flea").file_name().unwrap(), "flea");
    }
}
