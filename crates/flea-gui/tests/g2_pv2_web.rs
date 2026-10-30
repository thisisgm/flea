//! The web part of the document previews as real GPUI entities: the code view, HTML, font and archive pages, each
//! driven through a fake backend that records every ask, with count assertions on asks, lines shaped and decodes.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[path = "g2_pv2_web/archive.rs"]
mod archive;
#[path = "g2_pv2_web/code.rs"]
mod code;
#[path = "g2_pv2_web/font.rs"]
mod font;
#[path = "g2_pv2_web/html.rs"]
mod html;
#[path = "g2_pv2_web/parts.rs"]
mod parts;

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-pv2-web")
}

/// A marked scratch root that removes itself when its test passes and stays behind for a failure.
pub struct Root(PathBuf);

impl std::ops::Deref for Root {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let parent = scratch().canonicalize().unwrap();
        let ours = self.0.is_absolute()
            && self.0.file_name().is_some()
            && self.0.join(".flea-test-sandbox").is_file()
            && self.0.parent().and_then(|path| path.canonicalize().ok()) == Some(parent);
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

pub fn root() -> Root {
    let parent = scratch();
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g2-pv2-web\n").unwrap();
    Root(root)
}

/// Writes a solid PNG of the size into the root, as the helper writes its still image, and answers its path.
pub fn png(root: &Root, name: &str, width: u32, height: u32) -> String {
    let path = root.join(name);
    image::RgbaImage::from_pixel(width, height, image::Rgba([200, 100, 50, 255]))
        .save(&path)
        .unwrap();
    path.to_str().unwrap().to_owned()
}
