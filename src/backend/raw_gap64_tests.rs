//! RAW admission is independent of the host MIME database and keeps explicit handlers.
use crate::backend::mime::Db;
use crate::backend::{aliases::Aliases, rawembed, thumbspec::Thumbnailers};

const WANTED: [(&str, &str); 14] = [
    ("cr2", "image/x-canon-cr2"),
    ("cr3", "image/x-canon-cr3"),
    ("nef", "image/x-nikon-nef"),
    ("nrw", "image/x-nikon-nrw"),
    ("arw", "image/x-sony-arw"),
    ("sr2", "image/x-sony-sr2"),
    ("srf", "image/x-sony-srf"),
    ("dng", "image/x-adobe-dng"),
    ("raf", "image/x-fuji-raf"),
    ("orf", "image/x-olympus-orf"),
    ("rw2", "image/x-panasonic-rw2"),
    ("pef", "image/x-pentax-pef"),
    ("srw", "image/x-samsung-srw"),
    ("iiq", "image/x-phaseone-iiq"),
];

#[test]
fn sol_gap64_fourteen_raw_suffixes_reach_mime_and_container_admission() {
    let database = Db::from_str("");
    for (suffix, expected) in WANTED {
        let name = format!("folder.with.dots/photo.{}", suffix.to_uppercase());
        assert_eq!(database.lookup(&name), Some(expected), "{suffix}");
        assert!(rawembed::is_raw_name(&name), "{suffix}");
    }
    for name in ["photo.iiq.exe", "photo.bin", "photo.srw/child"] {
        assert_eq!(database.lookup(name), None, "{name}");
        assert!(!rawembed::is_raw_name(name), "{name}");
    }
    assert_eq!(
        Db::from_str("50:image/x-private:*.srw\n").lookup("photo.srw"),
        Some("image/x-private")
    );
    assert_eq!(
        Db::from_str("50:application/octet-stream:*.iiq\n").lookup("photo.iiq"),
        Some("image/x-phaseone-iiq")
    );
}

#[test]
fn sol_gap64_raw_dotfiles_keep_existing_container_admission() {
    let database = Db::from_str("");
    for (suffix, _) in WANTED {
        let name = format!("folder/.{}", suffix.to_uppercase());
        assert_eq!(database.lookup(&name), None, "no extension-only MIME guess");
        assert!(
            rawembed::is_raw_name(&name),
            "RAW reader still admits {name}"
        );
    }
}

#[test]
fn sol_gap64_raw_thumbnail_routes_keep_explicit_handlers_and_iiq_tiff_input() {
    let aliases = Aliases::from_str("");
    let specs = Thumbnailers::from_entries(
        &[
            (
                "private.thumbnailer".into(),
                "[Thumbnailer Entry]\nExec=/bin/sh jpeg %i %o\nMimeType=image/jpeg;\n".into(),
            ),
            (
                "tiff.thumbnailer".into(),
                "[Thumbnailer Entry]\nExec=/bin/sh tiff %i %o\nMimeType=image/tiff;\n".into(),
            ),
        ],
        &aliases,
    );
    for (suffix, mime) in WANTED {
        let route = specs
            .raw_route(mime, &aliases)
            .unwrap_or_else(|| panic!("missing {suffix}"));
        assert_eq!(route.exec[1], if suffix == "iiq" { "tiff" } else { "jpeg" });
    }
    assert!(specs.for_mime("image/x-not-camera-raw", &aliases).is_none());
    let own = Thumbnailers::from_entries(
        &[(
            "own.thumbnailer".into(),
            "[Thumbnailer Entry]\nExec=/bin/sh own %i %o\nMimeType=image/x-samsung-srw;\n".into(),
        )],
        &aliases,
    );
    assert!(own.raw_route("image/x-samsung-srw", &aliases).is_none());
    assert_eq!(
        own.for_mime("image/x-samsung-srw", &aliases).unwrap().exec[1],
        "own"
    );
}

#[test]
fn sol_gap64_iiq_thumbnailer_gets_the_original_tiff_input_instead_of_a_jpeg_copy() {
    let dir = crate::backend::testdir::TestDir::new("gap64-iiq-input");
    let input = dir.file("preview.iiq", "II*");
    let aliases = Aliases::from_str("");
    let specs = Thumbnailers::from_entries(
        &[(
            "tiff.thumbnailer".into(),
            "[Thumbnailer Entry]\nExec=/bin/sh tiff %i %o\nMimeType=image/tiff;\n".into(),
        )],
        &aliases,
    );
    let tables = crate::backend::thumbs::Tables {
        aliases: std::sync::Arc::new(aliases),
        specs: std::sync::Arc::new(specs),
        cache: crate::backend::thumbcache::Cache::at(dir.join("cache")),
        worker: None,
    };
    let job = crate::backend::thumbs::Job {
        path: input.clone(),
        mtime: 0,
        replies: None,
        mime: "image/x-phaseone-iiq".into(),
        source_size: None,
        trash_identity: None,
        trace: None,
    };
    assert_eq!(
        crate::backend::thumbs::input_for(&tables, &job, &dir.join("raw")),
        Some(input)
    );
    assert!(
        !dir.join("raw").exists(),
        "the TIFF route does not create a bogus embedded-JPEG cache"
    );
}
