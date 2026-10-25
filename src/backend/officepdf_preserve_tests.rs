use super::*;

#[test]
fn foreign_and_newer_office_documents_survive_lookup_and_first_publish() {
    let d = TestDir::new("office-preserve");
    let store = Store::new(d.path());
    for target in [store.pdf_path("key"), store.fail_path("key")] {
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        for original in ["{\"foreign\":true}", "{\"schemaVersion\":999}", "\0unparsable"] {
            std::fs::write(&target, original).unwrap();
            let _ = store.lookup("key", SystemTime::UNIX_EPOCH);
            assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
            assert!(Store::publish(&target, SystemTime::now(), &mut b"%PDF-1.4 new".as_slice()).unwrap_err().to_string().contains("left unchanged"));
            store.prune(target.parent().unwrap());
            assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
        }
    }
}

#[test]
fn a_symlink_planted_at_the_cache_path_is_not_followed() {
    let d = TestDir::new("officepdf-symlink");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let decoy = d.file("decoy.pdf", "decoy");
    std::fs::OpenOptions::new().write(true).open(&decoy).unwrap().set_modified(mtime_of(&src)).unwrap();
    let pdf = r.svc.store.pdf_path(&Store::key(&src));
    std::fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&decoy, &pdf).unwrap();
    assert_eq!(r.ask(1, &src), converting_line(1), "a planted link is a miss even when its target matches");
    assert!(r.end().1.contains("left unchanged"));
    assert!(std::fs::symlink_metadata(&pdf).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&decoy).unwrap(), b"decoy", "the target was never written");
}
