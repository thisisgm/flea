// A merge onto a FAT destination lands a new name inside the folder under the name Rename gives it.
use super::*;
use crate::backend::durable::EINVAL;
use crate::backend::fatnames::Choice;
use crate::backend::opsdispatch::Ops;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

// Stands the destination as FAT and its refused names, and clears both on the way out, a failed assert included.
struct StoodFat(PathBuf);
impl StoodFat {
    fn at(root: &Path, refused: &[(&str, i32)]) -> StoodFat {
        crate::backend::fatnames::test_stand_fat(root, true);
        crate::backend::unheld::test_refuse(refused);
        StoodFat(root.to_path_buf())
    }
}
impl Drop for StoodFat {
    fn drop(&mut self) {
        crate::backend::fatnames::test_stand_fat(&self.0, false);
        crate::backend::unheld::test_refuse(&[]);
    }
}

// Sample input: from/trip holds ok.txt, "clip: x.mp4" and "sub: a/b?.txt"; to/trip holds only ok.txt, so the other two are new.
#[test]
fn a_merge_lands_a_new_name_inside_the_folder_under_its_renamed_form() {
    let d = TestDir::new("merge-fat-rename");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.file("from/trip/ok.txt", "yours");
    d.file("from/trip/clip: x.mp4", "inside");
    d.dir("from/trip/sub: a");
    d.file("from/trip/sub: a/b?.txt", "deeper");
    d.dir("to/trip");
    d.file("to/trip/ok.txt", "there");
    let _stood = StoodFat::at(&to, &[("clip: x.mp4", EINVAL), ("sub: a", EINVAL), ("b?.txt", EINVAL)]);
    let ask = Ask { word: Some("merge".to_string()), id: 7, inner: Some("keep".to_string()), fat: Some(Choice::Rename) };
    let policy = ask.policy(Some(question_for(7, &[&trip], &to)), &to);
    let (ok, failed, _, errors, _, _, _, _) = transfer(false, &[&trip], &to, policy);
    assert_eq!((ok, failed), (1, 0), "{errors:?}");
    assert_eq!(text(&to.join("trip/clip_ x.mp4")), "inside");
    assert_eq!(text(&to.join("trip/sub_ a/b_.txt")), "deeper");
    assert_eq!(text(&to.join("trip/ok.txt")), "there", "the name both sides hold takes the second card's answer");
    assert!(to.join("trip/clip: x.mp4").symlink_metadata().is_err(), "no raw name landed");
}

// The next question a thread asked, landed on ops the way the loop does, and the line it wrote.
fn next_line(ops: &mut Ops, rx: &Receiver<OpMsg>) -> String {
    loop {
        let msg = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("a question answer");
        if matches!(msg, OpMsg::Asked { .. }) {
            let mut out = Vec::new();
            crate::backend::opsdispatch::report_op(&mut out, ops, msg);
            return String::from_utf8_lossy(&out).trim_end().to_string();
        }
    }
}

// Sample input: from/trip holds "clip: x.mp4" and to/trip already holds "clip_ x.mp4", the name Rename gives it.
#[test]
fn a_renamed_name_that_meets_an_existing_one_gets_the_second_cards_question() {
    let d = TestDir::new("merge-fat-ask");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.file("from/trip/clip: x.mp4", "inside");
    d.dir("to/trip");
    d.file("to/trip/clip_ x.mp4", "there");
    let _stood = StoodFat::at(&to, &[("clip: x.mp4", EINVAL)]);
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let (db, icons) = (Arc::new(crate::backend::mime::Db::load()), Arc::new(crate::backend::icons::Names::load()));
    let dest = to.to_string_lossy().to_string();
    crate::backend::collide::ask_beside(&mut ops, 7, 0, owned(&[&trip]), &dest, Some(Choice::Rename), (&db, &icons));
    next_line(&mut ops, &rx);
    ask_beside(&mut ops, 7, 0, owned(&[&trip]), &dest, &db, &icons);
    let line = next_line(&mut ops, &rx);
    assert!(line.contains(r#""total":1,"names":[{"n":"clip_ x.mp4""#), "the renamed name meets the one there: {line}");
    let ask = Ask { word: Some("merge".to_string()), id: 7, inner: Some("keep".to_string()), fat: Some(Choice::Rename) };
    let policy = ask.policy(ops.question.take(), &to);
    let (ok, failed, _, errors, _, _, kept, _) = transfer(false, &[&trip], &to, policy);
    assert_eq!((ok, failed, kept), (1, 0, 1), "{errors:?}");
    assert_eq!(text(&to.join("trip/clip_ x.mp4")), "there", "the name there is kept");
    assert_eq!(std::fs::read_dir(to.join("trip")).unwrap().count(), 2, "the second card's Keep both landed the other beside it");
}

// Sample input: trip merges into to/trip while a.txt and A.txt take one name in a folding to, which the first card asked about.
#[test]
fn the_merge_card_keeps_the_twins_the_first_card_asked_about() {
    let d = TestDir::new("merge-case-twins");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.file("from/trip/ok.txt", "yours");
    d.dir("to/trip");
    let (lower, upper) = (d.file("from/a.txt", "lower"), d.file("from/A.txt", "UPPER"));
    crate::backend::fatnames::casefold::test_stand_folding(&to, true);
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let (db, icons) = (Arc::new(crate::backend::mime::Db::load()), Arc::new(crate::backend::icons::Names::load()));
    let (dest, paths) = (to.to_string_lossy().to_string(), owned(&[&trip, &lower, &upper]));
    crate::backend::collide::ask_beside(&mut ops, 9, 0, paths.clone(), &dest, None, (&db, &icons));
    next_line(&mut ops, &rx);
    ask_beside(&mut ops, 9, 0, paths, &dest, &db, &icons);
    next_line(&mut ops, &rx);
    crate::backend::fatnames::casefold::test_stand_folding(&to, false);
    let question = ops.question.take().expect("the merge card's question");
    assert_eq!(question.twins.iter().collect::<Vec<_>>(), vec![&upper], "the merge card answers the twin too");
}
