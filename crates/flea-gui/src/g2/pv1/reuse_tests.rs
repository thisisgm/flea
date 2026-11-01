//! A JPEG's first Quick Look decode is the refinement's own picture, so one open reads the file once.
use crate::preview_quality::reuse;

#[test]
fn preview_quality_reuse_jpeg_first_decode_is_offered_as_the_refinement() {
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(std::env::temp_dir().join("flea-preview-reuse.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
    std::fs::write(root.join(".flea-test-sandbox"), "PQ-PERF").unwrap();
    let path = root.join("photo.jpg");
    image::RgbImage::from_fn(1200, 800, |x, y| {
        let v = if (x / 3 + y / 5) % 2 == 0 { 230 } else { 25 };
        image::Rgb([v, 255 - v, (x % 256) as u8])
    })
    .save(&path)
    .unwrap();
    let first = super::photo::decode(&path, (300, 200), &|| true).unwrap();
    assert_eq!(
        reuse::decodes(&path),
        1,
        "the first picture reads the file once"
    );
    let offered = reuse::take(&path, (300, 200)).expect("a sharp first decode is offered");
    assert!(std::sync::Arc::ptr_eq(&first, &offered));
    assert!(
        reuse::take(&path, (300, 200)).is_none(),
        "an offer is claimed once"
    );
    let refined = super::photo::decode_sharp(&path, (300, 200), &|| true).unwrap();
    assert_eq!(
        first.as_bytes(0),
        refined.as_bytes(0),
        "the offer is the refinement's pixels"
    );
    let other_size = super::photo::decode(&path, (299, 200), &|| true).unwrap();
    assert!(
        reuse::take(&path, (300, 200)).is_none(),
        "another size is never claimed"
    );
    assert!(std::sync::Arc::ptr_eq(
        &other_size,
        &reuse::take(&path, (299, 200)).unwrap()
    ));
    let stale = super::photo::decode(&path, (300, 200), &|| true).unwrap();
    image::RgbImage::from_pixel(1200, 800, image::Rgb([255, 0, 0]))
        .save(&path)
        .unwrap();
    assert!(
        reuse::take(&path, (300, 200)).is_none(),
        "a rewritten path cannot claim an older offer"
    );
    drop(stale);
    let meta = path.metadata().unwrap();
    let modified = meta.modified().unwrap();
    let version = (meta.len(), 4);
    let identity = reuse::file_identity(&path);
    reuse::keep(&path, version, (300, 200), &first, identity);
    assert!(reuse::kept(&path, version).is_some());
    let replacement = root.join("replacement.jpg");
    std::fs::write(&replacement, vec![0; meta.len() as usize]).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    std::fs::rename(replacement, &path).unwrap();
    assert_ne!(
        reuse::file_identity(&path),
        identity,
        "the fixture supplies a distinct identity with the same row version"
    );
    reuse::keep(&path, version, (300, 200), &first, identity);
    assert!(
        reuse::kept(&path, version).is_none(),
        "a changed identity cannot stamp older pixels with an unchanged row version"
    );
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn preview_quality_reuse_keeps_the_newest_refined_picture_of_each_file_version() {
    let picture = |w: u32, h: u32| {
        std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(w, h),
        )]))
    };
    let path = std::path::Path::new("/nonexistent/pq-perf-kept/photo.jpg");
    let (version, first, newer) = ((17, 4), picture(41, 27), picture(64, 43));
    reuse::keep(path, version, (41, 27), &first, reuse::file_identity(path));
    let (size, image) = reuse::kept(path, version).expect("a refined picture is kept");
    assert!(size == (41, 27) && std::sync::Arc::ptr_eq(&image, &first));
    assert!(
        reuse::kept(path, (17, 5)).is_none(),
        "a rewritten file never draws its old picture"
    );
    reuse::keep(path, version, (64, 43), &newer, reuse::file_identity(path));
    let (size, image) = reuse::kept(path, version).unwrap();
    assert!(size == (64, 43) && std::sync::Arc::ptr_eq(&image, &newer));
    let large = std::path::Path::new("/nonexistent/pq-perf-kept/large.jpg");
    reuse::keep(
        large,
        version,
        (1500, 1500),
        &picture(1500, 1500),
        reuse::file_identity(large),
    );
    assert!(
        reuse::kept(large, version).is_none(),
        "a picture over half the budget is not kept"
    );
    let others: Vec<_> = (0..8)
        .map(|n| std::path::PathBuf::from(format!("/nonexistent/pq-perf-kept/{n}.jpg")))
        .collect();
    for other in &others {
        reuse::keep(
            other,
            version,
            (8, 8),
            &picture(8, 8),
            reuse::file_identity(other),
        );
    }
    assert!(
        reuse::kept(path, version).is_none(),
        "the oldest leaves after eight newer pictures"
    );
    assert!(reuse::kept(&others[7], version).is_some());
}

#[test]
fn preview_quality_reuse_counts_every_animation_frame_in_its_budget() {
    let path = std::path::Path::new("/nonexistent/pq-perf-kept/animated.gif");
    let frames: Vec<_> = (0..12)
        .map(|_| image::Frame::new(image::RgbaImage::new(512, 512)))
        .collect();
    let image = std::sync::Arc::new(gpui::RenderImage::new(frames));
    reuse::keep(
        path,
        (17, 4),
        (512, 512),
        &image,
        reuse::file_identity(path),
    );
    assert!(
        reuse::kept(path, (17, 4)).is_none(),
        "all animation frames count towards the per-picture limit"
    );
}
