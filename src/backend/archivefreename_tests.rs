// Soak S1B-3 and S1B-5: a taken output name steps to the next copy name, and a climbing member is refused in a sentence.
use super::*;
use crate::backend::testdir::TestDir;

fn tar_tool(formats: &Formats) -> bool {
    !crate::backend::sandboxprobe::skipped() && formats.offers("tar.gz")
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with(".flea"))
        .collect();
    out.sort();
    out
}

#[test]
fn a_taken_name_steps_to_the_copy_names_duplicate_uses() {
    let at = Path::new("/d/pack.tar.gz");
    assert_eq!(
        freename::candidate(at, Some("tar.gz"), 1).unwrap(),
        Path::new("/d/pack copy.tar.gz")
    );
    assert_eq!(
        freename::candidate(at, Some("tar.gz"), 2).unwrap(),
        Path::new("/d/pack copy 2.tar.gz")
    );
    // A typed name without the format's tail splits its last dot, as Duplicate's own copy name does.
    assert_eq!(
        freename::candidate(Path::new("/d/notes.v2"), Some("zip"), 1).unwrap(),
        Path::new("/d/notes copy.v2")
    );
    // A folder keeps its whole name, dots and all.
    assert_eq!(
        freename::candidate(Path::new("/d/v1.2"), None, 1).unwrap(),
        Path::new("/d/v1.2 copy")
    );
    assert_eq!(
        freename::candidate(Path::new("/d/plain"), None, 3).unwrap(),
        Path::new("/d/plain copy 3")
    );
}

#[test]
fn a_second_compress_of_one_item_writes_the_next_copy_name() {
    let formats = Formats::from_tools(true, false);
    if !tar_tool(&formats) {
        return;
    }
    let d = TestDir::new("archfreecompress");
    d.dir("pack");
    d.file("pack/a.txt", "alpha");
    let names_in = vec!["pack".to_string()];
    let cancel = AtomicBool::new(false);
    for _ in 0..3 {
        compress(
            &formats,
            d.path(),
            &names_in,
            "tar.gz",
            &d.join("pack.tar.gz"),
            &cancel,
            None,
        )
        .expect("a taken name is no refusal");
    }
    assert_eq!(
        names(d.path()),
        [
            "pack",
            "pack copy 2.tar.gz",
            "pack copy.tar.gz",
            "pack.tar.gz"
        ]
    );
}

#[test]
fn an_extract_whose_folder_exists_writes_the_next_copy_folder_and_keeps_the_first() {
    let formats = Formats::from_tools(true, false);
    if !tar_tool(&formats) {
        return;
    }
    let d = TestDir::new("archfreeextract");
    d.dir("src");
    d.file("src/a.txt", "alpha");
    let cancel = AtomicBool::new(false);
    compress(
        &formats,
        &d.join("src"),
        &["a.txt".to_string()],
        "tar.gz",
        &d.join("proj.tar.gz"),
        &cancel,
        None,
    )
    .unwrap();
    compress(
        &formats,
        &d.join("src"),
        &["a.txt".to_string()],
        "tar",
        &d.join("proj.tar"),
        &cancel,
        None,
    )
    .unwrap();
    d.dir("proj");
    d.file("proj/mine.txt", "the operator's own file");
    // Two archives with one stem, the open case FX-ARCH left: each lands in its own folder.
    extract(
        &formats,
        &d.join("proj.tar.gz"),
        &d.join("proj"),
        &cancel,
        None,
    )
    .expect("a taken folder is no refusal");
    extract(
        &formats,
        &d.join("proj.tar"),
        &d.join("proj"),
        &cancel,
        None,
    )
    .expect("nor is a second one");
    assert_eq!(
        names(&d.join("proj")),
        ["mine.txt"],
        "nothing merges into the folder already there"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("proj copy/a.txt")).unwrap(),
        "alpha"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("proj copy 2/a.txt")).unwrap(),
        "alpha"
    );
}

#[test]
fn a_name_taken_while_the_tool_ran_is_passed_over_at_publish() {
    let d = TestDir::new("archfreerace");
    let staged = d.file("staged", "new");
    d.file("out.zip", "first");
    d.file("out copy.zip", "raced");
    let landed = freename::publish(
        &staged,
        &d.join("out.zip"),
        Some("zip"),
        &AtomicBool::new(false),
        |from, to, cancel| {
            // The check saw the name free; another writer took it before the real no-replace rename ran.
            if to == d.join("out copy 2.zip") && !d.join("out copy 3.zip").exists() {
                std::fs::write(d.join("out copy 3.zip"), "marker").unwrap();
                std::fs::write(to, "raced too").unwrap();
            }
            publish(from, to, cancel)
        },
    )
    .unwrap();
    assert_eq!(landed, d.join("out copy 4.zip"));
    assert_eq!(std::fs::read_to_string(&landed).unwrap(), "new");
    assert_eq!(std::fs::read_to_string(d.join("out.zip")).unwrap(), "first");
    assert_eq!(
        std::fs::read_to_string(d.join("out copy.zip")).unwrap(),
        "raced"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("out copy 2.zip")).unwrap(),
        "raced too"
    );
}

#[test]
fn a_taken_name_with_no_copy_name_says_so_instead_of_every_name_taken() {
    let d = TestDir::new("archfreenoname");
    let staged = d.file("staged", "new");
    let e = freename::publish(
        &staged,
        Path::new("/"),
        None,
        &AtomicBool::new(false),
        |_, _, _| panic!("no rename runs when no copy name exists"),
    )
    .unwrap_err();
    assert_eq!(e.msg, freename::NO_COPY_NAME);
    assert!(staged.is_file());
}

#[test]
fn a_taken_name_that_is_not_utf8_steps_to_a_copy_of_its_own_bytes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let raw = |bytes: &[u8]| Path::new("/d").join(OsStr::from_bytes(bytes));
    assert_eq!(
        freename::candidate(&raw(b"caf\xe9"), None, 1).unwrap(),
        raw(b"caf\xe9 copy")
    );
    assert_eq!(
        freename::candidate(&raw(b"caf\xe9.tar.gz"), Some("tar.gz"), 2).unwrap(),
        raw(b"caf\xe9 copy 2.tar.gz")
    );
    assert_eq!(
        freename::candidate(&raw(b"caf\xe9.v2"), Some("zip"), 1).unwrap(),
        raw(b"caf\xe9 copy.v2")
    );
    let d = TestDir::new("archfreenoutf8");
    let staged = d.file("staged", "new");
    let taken = d.path().join(OsStr::from_bytes(b"caf\xe9"));
    std::fs::create_dir(&taken).unwrap();
    let landed = freename::publish(
        &staged,
        &taken,
        None,
        &AtomicBool::new(false),
        |from, to, _| {
            std::fs::rename(from, to).map_err(|e| crate::error::from_io("archive", "", &e))
        },
    )
    .unwrap();
    assert_eq!(landed, d.path().join(OsStr::from_bytes(b"caf\xe9 copy")));
    assert_eq!(std::fs::read_to_string(&landed).unwrap(), "new");
    assert!(
        !d.path().join("caf\u{fffd} copy").exists(),
        "never the lossy twin"
    );
}

#[test]
fn a_member_that_climbs_out_is_refused_in_a_sentence_and_nothing_lands() {
    let formats = Formats::from_tools(true, false);
    if !tar_tool(&formats) {
        return;
    }
    let d = TestDir::new("archfreedotdot");
    let zip = crate::backend::model3d::testdata::archive(&[
        ("ok.txt", b"ok"),
        ("../../escape-dotdot.txt", b"out"),
    ]);
    std::fs::write(d.join("dotdot.zip"), zip).unwrap();
    let e = extract(
        &formats,
        &d.join("dotdot.zip"),
        &d.join("dotdot"),
        &AtomicBool::new(false),
        None,
    )
    .expect_err("bsdtar refuses the climbing member");
    assert_eq!(e.msg, ESCAPE_REFUSAL);
    assert!(!d.join("dotdot").exists());
    assert!(!d
        .path()
        .parent()
        .unwrap()
        .join("escape-dotdot.txt")
        .exists());
}

#[test]
fn only_bsdtars_secure_refusals_read_as_the_escape_sentence() {
    assert_eq!(
        escape_refusal("../../escape-dotdot.txt: Path contains '..': Unknown error -1"),
        Some(ESCAPE_REFUSAL)
    );
    assert_eq!(
        escape_refusal("link/evil.txt: Cannot extract through symlink link/evil.txt: Too many levels of symbolic links"),
        Some(ESCAPE_REFUSAL)
    );
    assert_eq!(escape_refusal("Damaged tar archive"), None);
}

// Moved from archivereq.rs, where it pinned the refusal this file replaces: a taken name is never written over.
#[test]
fn a_destination_already_there_is_never_written_over() {
    let f = Formats::from_tools(true, false);
    if crate::backend::sandboxprobe::skipped() || !f.offers("tar") {
        return;
    }
    let d = TestDir::new("archrefuse");
    d.file("a.txt", "alpha");
    d.file("out.tar", "already here");
    compress(
        &f,
        d.path(),
        &["a.txt".to_string()],
        "tar",
        &d.join("out.tar"),
        &AtomicBool::new(false),
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(d.join("out.tar")).unwrap(),
        "already here"
    );
    assert!(d.join("out copy.tar").is_file());

    d.dir("out");
    d.file("out/mine.txt", "mine");
    extract(
        &f,
        &d.join("out copy.tar"),
        &d.join("out"),
        &AtomicBool::new(false),
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_dir(d.join("out")).unwrap().count(),
        1,
        "merging into a directory in use is the surprise this rules out"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("out copy/a.txt")).unwrap(),
        "alpha"
    );
}
