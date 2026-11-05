use super::*;
use crate::backend::aliases::Aliases;

fn al() -> Aliases {
    Aliases::from_str("image/heic image/heif\nvideo/x-matroska video/matroska\n")
}

fn one(body: &str) -> Thumbnailers {
    Thumbnailers::from_entries(&[("t.thumbnailer".to_string(), body.to_string())], &al())
}

#[test]
fn a_declared_type_finds_its_spec() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -i %i -o %o -s %s\nMimeType=video/mp4;video/webm;\n");
    assert!(t.for_mime("video/mp4", &a).is_some());
    assert!(t.for_mime("video/webm", &a).is_some());
    assert!(t.for_mime("image/jpeg", &a).is_none());
}

// These thirteen RAW types use embedded JPEGs; IIQ uses the TIFF thumbnailer.
const JPEG_RAW: [&str; 13] = [
    "image/x-canon-cr2",
    "image/x-canon-cr3",
    "image/x-nikon-nef",
    "image/x-nikon-nrw",
    "image/x-sony-arw",
    "image/x-sony-sr2",
    "image/x-sony-srf",
    "image/x-adobe-dng",
    "image/x-fuji-raf",
    "image/x-olympus-orf",
    "image/x-panasonic-rw2",
    "image/x-pentax-pef",
    "image/x-samsung-srw",
];

#[test]
fn each_camera_raw_type_takes_the_jpeg_spec_and_only_when_one_is_installed() {
    let a = Aliases::from_str("image/x-panasonic-raw2 image/x-panasonic-rw2\n");
    let jpeg =
        one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -j %i %o\nMimeType=image/jpeg;\n");
    for m in JPEG_RAW.iter().chain(&["image/x-panasonic-raw2"]) {
        assert_eq!(
            jpeg.raw_route(m, &a).map(|s| s.exec[1].as_str()),
            Some("-j"),
            "{m}"
        );
        assert!(jpeg.for_mime(m, &a).is_some(), "{m}");
    }
    // IIQ is RAW, but a JPEG-only thumbnailer cannot serve its TIFF route.
    for m in ["image/png", "image/x-phaseone-iiq", "nonsense/nothing"] {
        assert!(
            jpeg.raw_route(m, &a).is_none() && jpeg.for_mime(m, &a).is_none(),
            "{m}"
        );
    }
    // Without an image/jpeg thumbnailer a RAW has none, as before.
    let video =
        one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=video/mp4;\n");
    assert!(JPEG_RAW.iter().all(|m| video.for_mime(m, &a).is_none()));
    // A thumbnailer that declares the RAW itself still serves it when no image/jpeg one exists, and the route stays off.
    let own = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -r %i %o\nMimeType=image/x-canon-cr2;\n");
    assert_eq!(
        own.for_mime("image/x-canon-cr2", &a)
            .map(|s| s.exec[1].as_str()),
        Some("-r")
    );
    assert!(own.raw_route("image/x-canon-cr2", &a).is_none());
}

#[test]
fn a_thumbnailer_that_declares_a_raw_keeps_it_when_an_image_jpeg_one_is_installed_too() {
    let a = Aliases::from_str("");
    let jpeg = (
        "jpeg.thumbnailer".to_string(),
        "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -j %i %o\nMimeType=image/jpeg;\n"
            .to_string(),
    );
    let cr2 = ("cr2.thumbnailer".to_string(), "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -r %i %o\nMimeType=image/x-canon-cr2;\n".to_string());
    let t = Thumbnailers::from_entries(&[jpeg, cr2], &a);
    // The declared CR2 is served by its own thumbnailer on the file itself, and the route stays off for it.
    assert_eq!(
        t.for_mime("image/x-canon-cr2", &a)
            .map(|s| s.exec[1].as_str()),
        Some("-r")
    );
    assert!(
        t.raw_route("image/x-canon-cr2", &a).is_none(),
        "the route must stay off for a declared RAW"
    );
    // A RAW nothing declares still takes the JPEG route.
    assert_eq!(
        t.for_mime("image/x-nikon-nef", &a)
            .map(|s| s.exec[1].as_str()),
        Some("-j")
    );
    assert_eq!(
        t.raw_route("image/x-nikon-nef", &a)
            .map(|s| s.exec[1].as_str()),
        Some("-j")
    );
}

#[test]
fn either_side_of_an_alias_pair_matches() {
    let a = al();
    // The declaration uses the alias and the query uses the canonical name.
    let t = one(
        "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=video/x-matroska;\n",
    );
    assert!(t.for_mime("video/matroska", &a).is_some());
    assert!(t.for_mime("video/x-matroska", &a).is_some());

    // And the other direction: the declaration uses the canonical name and the query uses the alias.
    let t2 =
        one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %u %o\nMimeType=image/heif;\n");
    assert!(t2.for_mime("image/heic", &a).is_some());
    assert!(t2.for_mime("image/heif", &a).is_some());
}

#[test]
fn a_missing_try_exec_is_allowed_and_the_exec_program_is_validated_instead() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nExec=/bin/sh %i %o\nMimeType=application/x-xoj;\n");
    assert!(t.for_mime("application/x-xoj", &a).is_some());
}

#[test]
fn an_unresolvable_try_exec_is_refused() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nTryExec=/definitely/not/here\nExec=/definitely/not/here %i %o\nMimeType=image/jpeg;\n");
    assert!(t.for_mime("image/jpeg", &a).is_none());
}

#[test]
fn a_try_exec_that_resolves_does_not_vouch_for_a_missing_exec_program() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nTryExec=/bin/true\nExec=/definitely/not/here %i %o\nMimeType=image/jpeg;\n");
    assert!(t.for_mime("image/jpeg", &a).is_none());
}

#[test]
fn a_key_outside_the_thumbnailer_entry_group_is_not_read() {
    let a = al();
    // The real Exec is inside the group; the one after it belongs to another group and must not win.
    let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=image/jpeg;\n[Other]\nExec=/definitely/not/here %i %o\n");
    assert_eq!(t.for_mime("image/jpeg", &a).unwrap().exec[0], "/bin/sh");
    // And a whole entry that never opens its group declares nothing at all.
    let t2 = one("TryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=image/jpeg;\n");
    assert!(t2.for_mime("image/jpeg", &a).is_none());
}

#[test]
fn a_directory_as_the_program_is_refused() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nTryExec=/tmp\nExec=/tmp %i %o\nMimeType=image/jpeg;\n");
    assert!(t.for_mime("image/jpeg", &a).is_none());
}

// Issue 171: the jail has no PATH, so load stores the path validation found and run never searches for a name.
#[test]
fn a_bare_exec_name_is_stored_as_the_path_that_validated() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nExec=sh %i %o\nMimeType=image/jpeg;\n");
    let program = &t.for_mime("image/jpeg", &a).unwrap().exec[0];
    assert!(
        program.starts_with('/') && program.ends_with("/sh"),
        "the program is {}",
        program
    );
    assert!(std::path::Path::new(program).is_file());
}

#[test]
fn a_bare_name_resolves_to_the_first_path_hit_that_is_a_runnable_file() {
    use std::os::unix::fs::PermissionsExt;
    let d = crate::backend::testdir::TestDir::new("thumbspec-resolve");
    let (first, second) = (d.join("first"), d.join("second"));
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    // The first hit is not executable, so the search goes on to the second.
    std::fs::write(first.join("tool"), b"#!/bin/sh\n").unwrap();
    std::fs::write(second.join("tool"), b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(second.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", first.display(), second.display());
    assert_eq!(
        resolve_in("tool", &path),
        Some(second.join("tool").to_string_lossy().to_string())
    );
    assert_eq!(
        resolve_in("tool", ""),
        None,
        "a name is not searched for without a PATH"
    );
    assert_eq!(resolve_in("/definitely/not/here", &path), None);
}

#[test]
fn a_relative_or_empty_path_entry_is_not_searched() {
    // A repository executable that a relative entry would find from the test's cwd.
    let here = std::env::current_dir().unwrap().join("tools");
    let absolute = here.join("flea-file-budget").to_string_lossy().to_string();
    assert_eq!(
        resolve_in("flea-file-budget", &here.to_string_lossy()),
        Some(absolute.clone()),
        "the control: an absolute entry finds it"
    );
    assert_eq!(resolve_in("flea-file-budget", "tools"), None);
    assert_eq!(resolve_in("flea-file-budget", ":./tools"), None);
    assert_eq!(
        resolve_in("flea-file-budget", &format!("tools:{}", here.display())),
        Some(absolute)
    );
}

#[test]
fn a_file_with_no_trailing_newline_keeps_its_last_field() {
    let a = al();
    let t = one(
        "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh %i %o\nMimeType=video/mp4;video/webm;",
    );
    assert!(t.for_mime("video/webm", &a).is_some());
}

#[test]
fn quoted_exec_tokens_survive_a_space() {
    let a = al();
    let t = one("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=\"/bin/sh\" --flag \"two words\" %i %o\nMimeType=image/jpeg;\n");
    let s = t.for_mime("image/jpeg", &a).unwrap();
    assert_eq!(s.exec[0], "/bin/sh");
    assert_eq!(s.exec[1], "--flag");
    assert_eq!(s.exec[2], "two words");
}

// Sample input, the MimeType field of a real /usr/share/thumbnailers file, shortened to the types asserted here; the programs stand in for evince, ffmpegthumbnailer and glycin because from_entries validates each against the live filesystem, so a real thumbnailer name asserts what the build machine has installed (issue 50).
fn shipped() -> Thumbnailers {
    let evince = "[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -s %s %u %o\nMimeType=application/pdf;image/tiff\n";
    let glycin = "[Thumbnailer Entry]\nTryExec=/bin/false\nExec=/bin/false --input %u --output %o --size %s\nMimeType=image/jpeg;image/heif;image/tiff\n";
    let ffmpeg = "[Thumbnailer Entry]\nTryExec=/bin/true\nExec=/bin/true -i %i -o %o -s %s -f\nMimeType=video/mp4;video/webm;video/matroska\n";
    Thumbnailers::from_entries(
        &[
            (
                "/usr/share/thumbnailers/evince.thumbnailer".to_string(),
                evince.to_string(),
            ),
            (
                "/usr/share/thumbnailers/ffmpegthumbnailer.thumbnailer".to_string(),
                ffmpeg.to_string(),
            ),
            (
                "/usr/share/thumbnailers/glycin-image-rs.thumbnailer".to_string(),
                glycin.to_string(),
            ),
        ],
        &al(),
    )
}

#[test]
fn the_heaviest_shipped_field_parses() {
    let a = al();
    let t = shipped();
    assert!(t.for_mime("image/jpeg", &a).is_some());
    assert!(t.for_mime("video/mp4", &a).is_some());
    assert!(t.for_mime("video/webm", &a).is_some());
    // The two alias cases Plan 3 found, from opposite sides.
    assert!(t.for_mime("image/heif", &a).is_some());
    assert!(t.for_mime("video/matroska", &a).is_some());
    assert!(t.for_mime("nonsense/nothing", &a).is_none());
}

#[test]
fn a_type_two_files_declare_goes_to_the_first_in_path_order() {
    // Both evince.thumbnailer and glycin-image-rs.thumbnailer declare image/tiff, and deleting the sort reddens this only while read_dir happens to return glycin first, which POSIX does not promise.
    assert_eq!(
        shipped().for_mime("image/tiff", &al()).unwrap().exec[0],
        "/bin/sh"
    );
}

// The only test here that reads this box's own directory, and it asserts the read rather than which thumbnailers are installed, because not one of them is even an optional dependency.
#[test]
fn the_live_directory_is_read_without_a_panic() {
    let a = Aliases::load();
    assert!(Thumbnailers::load(&a)
        .for_mime("nonsense/nothing", &a)
        .is_none());
}

#[test]
fn a_directorys_files_are_ordered_by_path_whatever_read_dir_returned() {
    let unsorted = vec![
        (
            "/usr/share/thumbnailers/glycin-image-rs.thumbnailer".to_string(),
            String::new(),
        ),
        (
            "/usr/share/thumbnailers/evince.thumbnailer".to_string(),
            String::new(),
        ),
    ];
    assert_eq!(
        sorted_by_path(unsorted)[0].0,
        "/usr/share/thumbnailers/evince.thumbnailer"
    );
}

// Sample output, a file name and a body that declares one type served by one program.
fn declared(mime: &str, program: &str) -> (String, String) {
    (
        format!("{}.thumbnailer", mime.replace('/', "-")),
        format!(
            "[Thumbnailer Entry]\nExec={} %i %o %s\nMimeType={};\n",
            program, mime
        ),
    )
}

#[test]
fn the_fingerprint_is_the_same_for_the_same_entries_in_any_order() {
    let a = al();
    let forward = [
        declared("image/png", "/bin/sh"),
        declared("video/mp4", "/bin/true"),
        declared("image/jpeg", "/bin/false"),
        declared("audio/flac", "/bin/true"),
    ];
    let reversed: Vec<(String, String)> = forward.iter().rev().cloned().collect();
    // Each table owns a HashMap with its own iteration order, so rebuilding is what exercises the sort.
    for _ in 0..20 {
        assert_eq!(
            Thumbnailers::from_entries(&forward, &a).fingerprint(),
            Thumbnailers::from_entries(&reversed, &a).fingerprint()
        );
    }
}

#[test]
fn the_fingerprint_changes_when_a_program_path_changes() {
    let a = al();
    let base = Thumbnailers::from_entries(
        &[
            declared("image/png", "/bin/true"),
            declared("video/mp4", "/bin/sh"),
        ],
        &a,
    );
    let moved = Thumbnailers::from_entries(
        &[
            declared("image/png", "/bin/false"),
            declared("video/mp4", "/bin/sh"),
        ],
        &a,
    );
    assert_ne!(base.fingerprint(), moved.fingerprint());
}

#[test]
fn the_fingerprint_changes_when_the_optional_jpeg_route_changes() {
    let mut specs = HashMap::new();
    specs.insert(JPEG_MIME.to_string(), Spec { exec: GLYCIN_JPEG_EXEC.map(String::from).to_vec() });
    assert_ne!(fingerprint_of(&specs, false), fingerprint_of(&specs, true));
}

#[test]
fn the_fingerprint_carries_the_embedded_field_codes_version() {
    // Failures recorded before embedded argv expansion hashed the entries alone, so the tag keeps them from matching.
    let empty = Thumbnailers::from_entries(&[], &al())
        .fingerprint()
        .to_string();
    let tagged = format!(
        "{:016x}",
        crate::backend::xxh64::hash(b"embedded-field-codes-v1\nliteral-imagemagick-paths-v1")
    );
    let untagged = format!("{:016x}", crate::backend::xxh64::hash(b""));
    assert_eq!(empty, tagged);
    assert_ne!(empty, untagged);
    assert_ne!(
        empty,
        format!(
            "{:016x}",
            crate::backend::xxh64::hash(b"embedded-field-codes-v1")
        )
    );
}
