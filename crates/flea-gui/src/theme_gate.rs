//! The GPUI theme gate reads the installed inventory, never a copied palette formula.
#[path = "theme_gate/contrast.rs"]
mod contrast;
pub(crate) mod frame;
mod isolation;
mod live;
mod quads;

pub(crate) fn stock_dir() -> std::path::PathBuf {
    let root = std::env::var_os("FLEA_THEME_STOCK_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/usr/share/omarchy/themes"));
    assert!(
        root.is_dir(),
        "no stock Omarchy themes: set FLEA_THEME_STOCK_DIR or install /usr/share/omarchy/themes ({} is missing)",
        root.display()
    );
    root
}

pub(crate) fn palettes() -> Vec<(String, String)> {
    let root = stock_dir();
    let mut palettes: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("stock palettes {}: {error}", root.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("colors.toml").is_file())
        .map(|path| {
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                std::fs::read_to_string(path.join("colors.toml")).unwrap(),
            )
        })
        .collect();
    palettes.sort_by(|a, b| a.0.cmp(&b.0));
    // The inventory is whatever the pinned Omarchy installs; only an empty one is refused.
    assert!(
        !palettes.is_empty(),
        "no stock palettes under {}",
        root.display()
    );
    for name in ["cool-dawn", "sunset"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tests/fixtures/{name}/colors.toml"));
        palettes.push((name.into(), std::fs::read_to_string(path).unwrap()));
    }
    palettes
}
