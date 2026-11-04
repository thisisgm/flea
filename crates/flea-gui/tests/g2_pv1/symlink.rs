//! A symbolic link to a large image reserves its bar; a plain copy fills the card.
use super::{boards::rect, pixels, scenes, *};

fn scratch() -> PathBuf {
    repo(".superpowers/fix-qlsymlink/fixtures")
}

/// A marked root holding photo.png and link.png; it removes itself when its test passes and stays behind for a failure.
struct Fixture {
    root: PathBuf,
    path: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = scratch().canonicalize().unwrap();
        let ours = self.root.is_absolute()
            && self.root.file_name().is_some()
            && self.root.join(".flea-test-sandbox").is_file()
            && self.root.parent().and_then(|path| path.canonicalize().ok()) == Some(parent);
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
}

fn fixture(linked: bool) -> Fixture {
    let parent = scratch();
    std::fs::create_dir_all(&parent).unwrap();
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("image.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(&parent));
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    let photo = root.join("photo.png");
    image::RgbaImage::new(800, 600).save(&photo).unwrap();
    let path = root.join("link.png");
    if linked {
        std::os::unix::fs::symlink("photo.png", &path).unwrap();
    } else {
        std::fs::copy(photo, &path).unwrap();
    }
    assert_eq!(
        std::fs::symlink_metadata(&path).unwrap().is_symlink(),
        linked
    );
    Fixture { root, path }
}

#[gpui::test]
fn a_large_symlink_image_fits_below_its_bar(cx: &mut TestAppContext) {
    let mut subject = scenes::ql_symlink(&crate::test_catalogs::english());
    let made = fixture(true);
    if let Body::Link { path, .. } = &mut subject.body {
        *path = made.path.clone();
    }
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    pixels::at_board_scale(cx);
    assert!(cx.debug_bounds("pv1-bar-mark:flea-mark:symlink").is_some());
    let bar = rect(cx, "pv1-bar");
    let picture = rect(cx, "pv1-picture");
    assert_eq!(picture, (185., 90., 729., 547.));
    assert!(picture.1 >= bar.1 + bar.3);
}

#[gpui::test]
fn a_plain_large_image_keeps_the_whole_card_without_a_bar(cx: &mut TestAppContext) {
    let mut subject = scenes::ql_image();
    subject.name = "link.png".into();
    let made = fixture(false);
    if let Body::Image { path, .. } = &mut subject.body {
        *path = made.path.clone();
    }
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    pixels::at_board_scale(cx);
    assert!(cx.debug_bounds("pv1-bar").is_none());
    let picture = rect(cx, "pv1-picture");
    assert_eq!(picture, (168., 63., 765., 574.));
}
