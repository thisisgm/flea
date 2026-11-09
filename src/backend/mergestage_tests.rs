// A merge into an existing folder stages each new file on its own and each new subfolder whole.
use super::*;

const ENOSPC: i32 = 28;
const CHUNK: usize = 256 * 1024;

// A new subfolder cut short inside a merge leaves no part of itself under its name; the folder it merged into stays.
#[test]
fn a_new_subfolder_cut_short_in_a_merge_leaves_nothing_under_its_name() {
    let d = TestDir::new("merge-stage-fail");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.dir("from/trip/newsub");
    let big = vec![7u8; 3 * CHUNK];
    std::fs::write(trip.join("newsub/a.bin"), &big).unwrap();
    std::fs::write(trip.join("newsub/b.bin"), &big).unwrap();
    d.dir("to/trip");
    d.file("to/trip/old.txt", "theirs");
    // One whole file and a chunk land, then the fifth write fails half written.
    crate::backend::copyfile::test_fail_write(Some((4, ENOSPC)));
    let (ok, failed, _, errors, ..) = transfer(false, &[&trip], &to, merged_policy(7, &[&trip], &to, "keep"));
    crate::backend::copyfile::test_fail_write(None);
    assert_eq!((ok, failed), (0, 1), "the merge reports its one failure");
    assert!(errors.iter().any(|e| e.contains("full")), "and it is the injected write: {errors:?}");
    assert!(!to.join("trip/newsub").exists(), "no part of the new subfolder took its name");
    let names: Vec<String> = std::fs::read_dir(to.join("trip")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["old.txt"], "nothing hidden stays beside what the folder held");
}

// A merge lands new files and a new subfolder whole beside what the folder held, with nothing hidden left.
#[test]
fn a_merge_lands_new_files_and_subfolders_whole() {
    let d = TestDir::new("merge-stage-ok");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.dir("from/trip/newsub");
    d.file("from/trip/new.txt", "new");
    d.file("from/trip/newsub/inner.txt", "inner");
    d.dir("to/trip");
    d.file("to/trip/old.txt", "theirs");
    let (ok, failed, ..) = transfer(false, &[&trip], &to, merged_policy(7, &[&trip], &to, "keep"));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(text(&to.join("trip/new.txt")), "new");
    assert_eq!(text(&to.join("trip/newsub/inner.txt")), "inner");
    assert_eq!(text(&to.join("trip/old.txt")), "theirs");
    let hidden = |dir: &std::path::Path| std::fs::read_dir(dir).unwrap().flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(crate::backend::copystage::PREFIX));
    assert!(!hidden(&to.join("trip")) && !hidden(&to.join("trip/newsub")));
}
