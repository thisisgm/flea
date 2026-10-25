use super::*;

#[test]
fn foreign_and_newer_preview_documents_survive_read_and_first_extract() {
    let d = TestDir::new("raw-preserve");
    let root = d.dir("cache");
    let source = raw_with(&d, "a.cr2", &jpeg(800, 600), None);
    let target = copy_path(&root, &source);
    for original in [
        "{\"foreign\":true}",
        "{\"schemaVersion\":999}",
        "unparsable",
    ] {
        std::fs::write(&target, original).unwrap();
        set_mtime(&target, 1);
        assert!(!fresh(
            &target,
            source.metadata().unwrap().modified().unwrap()
        ));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
        assert!(ensure_in(&root, &source).is_none());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
    }
}

#[test]
fn a_symlink_planted_at_the_cache_path_is_preserved_and_never_written_through() {
    let d = TestDir::new("rawprev-link");
    let cache = d.join("cache");
    let src = raw_with(&d, "b.cr2", &jpeg(640, 480), None);
    set_mtime(&src, 1_700_000_000);
    let victim = d.file("victim.jpg", "keep me");
    set_mtime(&victim, 1_700_000_000);
    std::fs::create_dir(&cache).unwrap();
    std::os::unix::fs::symlink(&victim, copy_path(&cache, &src)).unwrap();
    assert!(ensure_in(&cache, &src).is_none());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
    assert!(std::fs::symlink_metadata(copy_path(&cache, &src))
        .unwrap()
        .file_type()
        .is_symlink());
}
