use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"))
        .join("../../ci/visual/scenes");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut paths: Vec<_> = fs::read_dir(root)
        .expect("scene manifest directory")
        .map(|entry| entry.expect("scene manifest entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "scene registry is empty");
    let entries = paths
        .iter()
        .map(|path| {
            format!(
                "include_str!({:?}),",
                path.to_str().expect("UTF-8 scene path")
            )
        })
        .collect::<String>();
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"));
    fs::write(output.join("visual-scenes.rs"), format!("&[{entries}]"))
        .expect("compiled scene registry");
}
