use super::*;
use crate::backend::testdir::TestDir;

// AA4: a disk image extract is refused before the slot and before any start line, so no job starts.
#[test]
fn an_iso_extract_answers_archivedone_and_starts_nothing() {
    let d = TestDir::new("archisostart");
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    start_archive(&mut out, &mut ops, Arc::new(Formats::from_tools(true, true)), "extract", crate::backend::archivereq::ArchiveRequest { compressing: false, paths: Vec::new(), format: "7z".into(), archive: d.join("debian-13.iso"), dest: d.join("debian-13"), password: String::new(), encrypt_names: false },
        0);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 1, "one terminal line and nothing before it: {}", text);
    assert!(text.contains(r#""t":"archivedone""#) && text.contains(r#""ok":false"#), "{}", text);
    assert!(text.contains("a disk image opens as a listing and is never extracted"), "{}", text);
    for started in ["archivestarted", "extractstarted", "transferprogress"] {
        assert!(!text.contains(started), "a refused extract announced {}: {}", started, text);
    }
    assert!(ops.live.running().is_none(), "a refused extract took the slot");
}

#[test]
fn an_archive_no_tool_reads_is_a_named_error_rather_than_a_silent_failure() {
    let d = TestDir::new("archnoreader");
    let none = Formats::from_tools(false, false);
    let e = extract(&none, &d.join("x.zip"), &d.join("out"), &AtomicBool::new(false), None).unwrap_err();
    assert_eq!(e.msg, "no installed tool can open that archive");
}

#[test]
fn items_from_two_folders_are_refused_in_plain_words() {
    let d = TestDir::new("archtwofolders");
    d.dir("one");
    d.dir("two");
    let first = d.file("one/a.txt", "a");
    let second = d.file("two/b.txt", "b");
    let (tx, rx) = std::sync::mpsc::channel();
    run_archive(7, crate::backend::archivereq::ArchiveRequest { compressing: true, paths: vec![first.to_string_lossy().to_string(), second.to_string_lossy().to_string()], format: "zip".into(), archive: PathBuf::new(), dest: d.join("out.zip"), password: String::new(), encrypt_names: false }, &Formats::from_tools(true, true), tx,
        None, Arc::new(AtomicBool::new(false)));
    match rx.recv().unwrap() {
        OpMsg::DetachedDone { line, .. } => assert!(line.contains(r#""err":"the items to compress must be in one folder""#), "{}", line),
        _ => panic!("a compress answers through DetachedDone"),
    }
    assert!(!d.join("out.zip").exists());
}
