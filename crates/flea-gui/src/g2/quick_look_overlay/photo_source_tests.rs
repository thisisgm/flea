//! One decode of a JPEG serves every size asked for while it is wanted, and nothing outlives the last request.
use super::photo_source::{hold, shared, Decoded};
use std::{
    fs::File,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

fn sandbox() -> PathBuf {
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(std::env::temp_dir().join("flea-photo-source.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
    std::fs::write(root.join(".flea-test-sandbox"), "PQ-PERF").unwrap();
    root
}

fn photo(path: &Path, width: u32) {
    image::RgbImage::from_fn(width, 400, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
    })
    .save(path)
    .unwrap();
}

fn read(file: File, count: &AtomicUsize) -> Result<Decoded, ()> {
    count.fetch_add(1, Ordering::SeqCst);
    let reader =
        image::ImageReader::with_format(std::io::BufReader::new(file), image::ImageFormat::Jpeg);
    Ok((
        reader.decode().map_err(|_| ())?,
        image::metadata::Orientation::NoTransforms,
    ))
}

#[test]
fn preview_quality_shared_decode_serves_a_resize_without_a_second_decode() {
    let root = sandbox();
    let path = root.join("photo.jpg");
    photo(&path, 600);
    let count = Arc::new(AtomicUsize::new(0));
    let (started, running) = mpsc::channel();
    let (open, gate) = mpsc::channel::<()>();
    let owner = std::thread::spawn({
        let (path, count) = (path.clone(), count.clone());
        move || {
            shared(&path, File::open(&path).unwrap(), &|| true, |file| {
                started.send(()).unwrap();
                gate.recv().unwrap();
                read(file, &count)
            })
        }
    });
    running.recv().unwrap();
    let queued = hold(&path).expect("a running decode can be held by a queued request");
    let wanted = Arc::new(AtomicBool::new(true));
    let leaving = std::thread::spawn({
        let (path, wanted) = (path.clone(), wanted.clone());
        move || {
            shared(
                &path,
                File::open(&path).unwrap(),
                &|| wanted.load(Ordering::SeqCst),
                |_| panic!("a waiter never decodes"),
            )
        }
    });
    wanted.store(false, Ordering::SeqCst);
    assert!(
        leaving.join().unwrap().is_err(),
        "a superseded waiter leaves before the decode ends"
    );
    let joiner = std::thread::spawn({
        let path = path.clone();
        move || {
            shared(&path, File::open(&path).unwrap(), &|| true, |_| {
                panic!("a resize joins the running decode")
            })
        }
    });
    open.send(()).unwrap();
    let (first, _, first_hold) = owner.join().unwrap().unwrap();
    let (joined, wanted, joined_hold) = joiner.join().unwrap().unwrap();
    assert!(Arc::ptr_eq(&first, &joined));
    assert!(wanted, "the queued request still holds the decode");
    drop((first, joined, first_hold, joined_hold));
    let (late, _, late_hold) = shared(&path, File::open(&path).unwrap(), &|| true, |_| {
        panic!("a held decode serves the request queued behind it")
    })
    .unwrap();
    assert_eq!(late.0.width(), 600);
    assert_eq!(count.load(Ordering::SeqCst), 1, "one decode for every size");
    photo(&path, 500);
    let (changed, wanted, changed_hold) =
        shared(&path, File::open(&path).unwrap(), &|| true, |file| {
            read(file, &count)
        })
        .unwrap();
    assert_eq!(changed.0.width(), 500, "a rewritten file is decoded again");
    assert!(
        !wanted,
        "no other request holds a decode only its owner asked for"
    );
    drop((queued, late, changed, late_hold, changed_hold));
    assert!(
        hold(&path).is_none(),
        "nothing is kept once no request wants it"
    );
    shared(&path, File::open(&path).unwrap(), &|| true, |file| {
        read(file, &count)
    })
    .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 3);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn preview_quality_shared_decode_retains_source_while_pixels_are_reduced() {
    let root = sandbox();
    let path = root.join("retained.jpg");
    photo(&path, 600);
    let count = AtomicUsize::new(0);
    let first = shared(&path, File::open(&path).unwrap(), &|| true, |file| {
        read(file, &count)
    })
    .unwrap();
    assert!(
        hold(&path).is_some(),
        "a resize can hold the source while its first reduction runs"
    );
    let second = shared(&path, File::open(&path).unwrap(), &|| true, |_| {
        panic!("a concurrent reduction must reuse the source")
    })
    .unwrap();
    assert!(Arc::ptr_eq(&first.0, &second.0));
    drop((first, second));
    assert!(hold(&path).is_none());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn preview_quality_shared_decode_rejects_rewrites_preserving_mtime() {
    let root = sandbox();
    let path = root.join("rewritten.jpg");
    std::fs::write(&path, b"AAAA").unwrap();
    let metadata = path.metadata().unwrap();
    let modified = metadata.modified().unwrap();
    let stamp = (metadata.ctime(), metadata.ctime_nsec());
    let pixels = |value| {
        Ok((
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                1,
                1,
                image::Rgb([value, 0, 0]),
            )),
            image::metadata::Orientation::NoTransforms,
        ))
    };
    let mut old = None;
    let first = shared(&path, File::open(&path).unwrap(), &|| true, |_| {
        old = hold(&path);
        pixels(1)
    })
    .unwrap();
    assert!(old.is_some());
    // Cross the filesystem's ctime tick while preserving inode, size and mtime.
    let start = Instant::now();
    loop {
        std::fs::write(&path, b"BBBB").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let metadata = path.metadata().unwrap();
        if (metadata.ctime(), metadata.ctime_nsec()) != stamp {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "fixture ctime did not advance"
        );
        std::thread::yield_now();
    }
    assert!(
        hold(&path).is_none(),
        "a queued resize must not hold the previous file identity"
    );
    let second = shared(&path, File::open(&path).unwrap(), &|| true, |_| pixels(2)).unwrap();
    assert!(
        !Arc::ptr_eq(&first.0, &second.0),
        "ctime invalidates equal-length rewrites with restored mtime"
    );
    drop((old, first, second));
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(root).unwrap();
}
