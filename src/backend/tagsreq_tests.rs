// The settags request's tests, beside it so tagsreq.rs stays under the file budget.
use super::*;
use crate::backend::dirsizeworker::Worker;
use crate::backend::testdir::TestDir;
use std::sync::mpsc::channel;

pub(super) fn state_for(base: &Path) -> State {
    let (tx, _rx) = crate::backend::events::channel();
    let mut st = State::new(Worker::new(tx));
    st.base = base.to_path_buf();
    st
}

pub(super) fn ops() -> crate::backend::opsdispatch::Ops {
    let (tx, _rx) = channel();
    crate::backend::opsdispatch::Ops::new(tx)
}

#[test]
fn a_write_changes_every_file_and_journals_each_previous_attribute() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqwrite");
    let _first = d.file("a.txt", "body");
    let _second = d.file("b.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    st.listing.push("b.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1], "add", "Red");
    assert_eq!(String::from_utf8(out).unwrap(), settagged_line(2, "add") + "\n");
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), vec!["Red"]);
    assert_eq!(o.journal.len(), 1);
    let mut out = Vec::new();
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), Vec::<String>::new());
    assert_eq!(crate::backend::tags::read(&d.join("b.txt")).unwrap(), Vec::<String>::new());
}

#[test]
fn a_file_that_vanished_refuses_all_and_writes_nothing() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqgone");
    let _first = d.file("a.txt", "body");
    let gone = d.file("gone.txt", "body");
    std::fs::remove_file(&gone).unwrap();
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    st.listing.push("gone.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1], "add", "Red");
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""where":"settags""#), "{}", line);
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), Vec::<String>::new());
    assert_eq!(o.journal.len(), 0);
}

#[test]
fn an_unsupported_filesystem_refuses_before_anything_is_written() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqrefuse");
    let _first = d.file("a.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    crate::backend::tags::FORCE_UNSUPPORTED.with(|f| f.set(true));
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "add", "Red");
    crate::backend::tags::FORCE_UNSUPPORTED.with(|f| f.set(false));
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""where":"settags""#), "{}", line);
    assert!(line.contains("nothing was changed"), "{}", line);
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), Vec::<String>::new());
    assert_eq!(o.journal.len(), 0);
}

#[test]
fn a_noop_change_answers_success_and_records_nothing() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqnoop");
    let _file = d.file("a.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "remove", "Red");
    assert_eq!(String::from_utf8(out).unwrap(), settagged_line(0, "remove") + "\n");
    assert_eq!(o.journal.len(), 0, "an operation that changed nothing records nothing");
}

#[test]
fn a_stale_numbering_is_refused_before_anything_resolves() {
    let line = r#"{"c":"settags","rows":[0],"op":"add","tag":"Red","listing":1}"#;
    let refused = crate::backend::rowguard::refusal(line, 2).expect("a stale settags is refused");
    assert!(refused.contains(r#""where":"stale""#), "{}", refused);
}

#[test]
fn a_failed_write_after_an_unchanged_file_puts_back_every_file_it_wrote() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqrollback");
    let already = d.file("a.txt", "body");
    let _plain = d.file("b.txt", "body");
    crate::backend::tags::write_names(&already, &["Red".to_string()]).unwrap();
    // A read-only file probes clean but refuses the write, so the third write fails after two rows were read.
    let locked = d.file("c.txt", "body");
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o444)).unwrap();
    let mut st = state_for(d.path());
    for name in ["a.txt", "b.txt", "c.txt"] {
        st.listing.push(name, false);
    }
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1, 2], "add", "Red");
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""where":"settags""#), "{}", line);
    assert_eq!(crate::backend::tags::read(&d.join("b.txt")).unwrap(), Vec::<String>::new(), "the written file was put back");
    assert_eq!(crate::backend::tags::read(&already).unwrap(), vec!["Red"], "the unchanged file kept its own tag");
    assert_eq!(o.journal.len(), 0);
}

#[test]
fn a_symlink_in_the_selection_is_skipped_and_the_rest_tagged() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqskiplink");
    let _first = d.file("a.txt", "body");
    let _second = d.file("b.txt", "body");
    // A link reads as untagged but Linux refuses a user attribute on it, so it is left out by name.
    std::os::unix::fs::symlink(d.join("b.txt"), d.join("c.lnk")).unwrap();
    let mut st = state_for(d.path());
    for name in ["a.txt", "b.txt", "c.lnk"] {
        st.listing.push(name, false);
    }
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1, 2], "add", "Red");
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""t":"settagged""#), "the rest still tag: {}", line);
    assert!(line.contains(r#""ok":2"#), "only the two files change: {}", line);
    assert!(line.contains(r#""skipped":["c.lnk"]"#), "the skipped link rides as data: {}", line);
    assert!(!line.contains(r#""note""#), "no English sentence rides the wire: {}", line);
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), vec!["Red"]);
    assert_eq!(crate::backend::tags::read(&d.join("b.txt")).unwrap(), vec!["Red"]);
    assert_eq!(o.journal.len(), 1);
}

#[test]
fn a_skipped_row_is_named_as_a_search_listing_shows_it() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqskipnested");
    d.dir("a");
    d.dir("b");
    let real = d.file("a/real.txt", "body");
    // A search row is its path under the base, so two links with one leaf name stay told apart.
    std::os::unix::fs::symlink(&real, d.join("a/x.lnk")).unwrap();
    std::os::unix::fs::symlink(&real, d.join("b/x.lnk")).unwrap();
    let mut st = state_for(d.path());
    for name in ["a/real.txt", "a/x.lnk", "b/x.lnk"] {
        st.listing.push(name, false);
    }
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1, 2], "add", "Red");
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""ok":1"#), "only the real file changes: {}", line);
    assert!(line.contains(r#""skipped":["a/x.lnk","b/x.lnk"]"#), "each link is named by its row path: {}", line);
}

#[test]
fn every_filesystem_the_targets_sit_on_is_probed_before_any_write() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqmixed");
    let _first = d.file("a.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    // An absolute name joins as itself, as a search row under another mount does; procfs refuses user attributes.
    st.listing.push("/proc/self/status", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1], "add", "Red");
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains("does not support file tags"), "the second filesystem refuses by name: {}", line);
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), Vec::<String>::new());
    assert_eq!(o.journal.len(), 0);
}

// The fake table maps the sandbox onto an nfs mount, so the write decision reads it as remote.
fn remote_body(dir: &Path) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", dir.to_string_lossy())
}

#[test]
fn a_remote_tag_change_goes_through_the_slow_write_bound() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::backend::iomount::test_reset();
    let d = TestDir::new("tagsreqremote");
    let file = d.file("a.txt", "body");
    let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "add", "Red");
    assert_eq!(String::from_utf8(out).unwrap(), settagged_line(1, "add") + "\n");
    assert!(crate::backend::iomount::test_calls() >= 1, "a remote write takes the bound rename and mkdir take");
    assert_eq!(crate::backend::tags::read(&file).unwrap(), vec!["Red"]);
    assert_eq!(o.journal.len(), 1);
}

#[test]
fn a_held_remote_tag_change_answers_slow_then_journals_when_it_lands() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqslow");
    let file = d.file("a.txt", "body");
    let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
    let (tx, rx) = channel();
    let mut o = crate::backend::opsdispatch::Ops::new(tx);
    let (release, wait) = channel::<()>();
    let paths = vec![file.clone()];
    let held = paths.clone();
    let base = d.path().to_path_buf();
    let mut out = Vec::new();
    // The worker writes only after the test releases it, so the loop answers at the deadline first.
    apply_tags_with(&mut out, &mut o, paths, std::time::Duration::from_millis(50), move || {
        let _ = wait.recv();
        write_all(&base, &held, "add", "Red")
    });
    let line = String::from_utf8(std::mem::take(&mut out)).unwrap();
    assert!(line.contains(r#""t":"slow""#) && line.contains(r#""op":"settags""#), "{}", line);
    assert!(line.contains(" is slow. The tag change will finish on its own."), "{}", line);
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    assert!(String::from_utf8(std::mem::take(&mut out)).unwrap().contains("already running"), "undo waits for the late entry");
    assert_eq!(o.journal.len(), 0, "nothing is journalled until the held write lands");
    drop(release);
    let landed = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the released write reports through the op channel");
    crate::backend::opsdispatch::report_op(&mut out, &mut o, landed);
    assert_eq!(String::from_utf8(std::mem::take(&mut out)).unwrap(), settagged_line(1, "add") + "\n");
    assert!(o.pending.is_empty(), "the late reply clears its own pending entry");
    assert_eq!(o.journal.len(), 1);
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    assert!(String::from_utf8(out).unwrap().contains(r#"{"t":"undone","op":"settags","ok":true}"#));
    assert_eq!(crate::backend::tags::read(&file).unwrap(), Vec::<String>::new());
}

// Three files in one listing, the first `carrying` of them already tagged Red, toggled Red together.
fn toggle_red_over_three(name: &str, carrying: usize) -> (TestDir, String) {
    let d = TestDir::new(name);
    let mut st = state_for(d.path());
    for (i, file) in ["a.txt", "b.txt", "c.txt"].iter().enumerate() {
        let path = d.file(file, "body");
        if i < carrying {
            crate::backend::tags::write_names(&path, &["Red".to_string()]).unwrap();
        }
        st.listing.push(file, false);
    }
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1, 2], "toggle", "red");
    (d, String::from_utf8(out).unwrap())
}

fn red_on(d: &TestDir) -> Vec<bool> {
    ["a.txt", "b.txt", "c.txt"].iter().map(|f| crate::backend::tags::read(&d.join(f)).unwrap() == vec!["Red"]).collect()
}

#[test]
fn a_toggle_where_every_target_carries_the_tag_removes_it_from_all() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (d, line) = toggle_red_over_three("tagsgroupall", 3);
    assert_eq!(line, settagged_line(3, "remove") + "\n");
    assert_eq!(red_on(&d), vec![false, false, false]);
}

#[test]
fn a_toggle_where_some_targets_carry_the_tag_adds_it_to_all() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (d, line) = toggle_red_over_three("tagsgroupsome", 1);
    assert_eq!(line, settagged_line(2, "add") + "\n", "only the two files without Red change");
    assert_eq!(red_on(&d), vec![true, true, true], "no file that carried Red loses it");
}

#[test]
fn a_toggle_where_no_target_carries_the_tag_adds_it_to_all() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (d, line) = toggle_red_over_three("tagsgroupnone", 0);
    assert_eq!(line, settagged_line(3, "add") + "\n");
    assert_eq!(red_on(&d), vec![true, true, true]);
}

#[test]
fn the_settagged_line_names_the_op_it_applied() {
    assert_eq!(settagged_line(2, "remove"), r#"{"t":"settagged","ok":2,"op":"remove"}"#);
    assert_eq!(settagged_skipped(2, "remove", &[]), settagged_line(2, "remove"));
    assert_eq!(resolved("clear", "", &[]), "clear");
    assert_eq!(resolved("toggle", "Red", &[]), "add", "a toggle over nothing adds");
    assert_eq!(resolved("toggle", "Red", &[vec!["red".to_string()]]), "remove", "names match without regard to case");
}

#[test]
fn a_row_that_is_a_remote_mount_root_is_written_off_the_loop() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqmountroot");
    // The sandbox itself is the remote mount, so the row's own mount is remote while its parent is local.
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", d.path().to_string_lossy());
    let _held = crate::backend::iomount::test_hold_body(body);
    let (tx, rx) = channel();
    let mut o = crate::backend::opsdispatch::Ops::new(tx);
    let (release, wait) = channel::<()>();
    let row = d.path().to_path_buf();
    let held = vec![row.clone()];
    let base = d.path().to_path_buf();
    // The releaser frees the worker, so an inline write would answer ready instead of slow.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = release.send(());
    });
    let mut out = Vec::new();
    // The worker writes only after the release, so the loop answers at the deadline first.
    apply_tags_with(&mut out, &mut o, vec![row], std::time::Duration::from_millis(50), move || {
        let _ = wait.recv();
        write_all(&base, &held, "add", "Red")
    });
    let line = String::from_utf8(std::mem::take(&mut out)).unwrap();
    assert!(line.contains(r#""t":"slow""#) && line.contains(r#""op":"settags""#), "the mount root write leaves the loop: {}", line);
    assert_eq!(o.pending.len(), 1, "the late write holds its own pending entry");
    let landed = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the released write reports through the op channel");
    crate::backend::opsdispatch::report_op(&mut out, &mut o, landed);
    assert_eq!(String::from_utf8(std::mem::take(&mut out)).unwrap(), settagged_line(1, "add") + "\n");
    assert!(o.pending.is_empty(), "the late reply clears its own pending entry");
    assert_eq!(crate::backend::tags::read(d.path()).unwrap(), vec!["Red"]);
}

#[test]
fn tagstates_on_a_stuck_mount_answers_without_a_reader_thread() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsstatesstuck");
    let a = d.file("a.txt", "body");
    crate::backend::tags::write_names(&a, &["Red".to_string()]).unwrap();
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    st.generation = 4;
    // The row's mount is marked stuck, so the read answers untagged with no reader thread.
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mount = crate::backend::iomount::mount_key(d.path(), &body);
    let _stuck = crate::backend::iomount::test_hold_stuck(mount);
    crate::backend::iomount::test_reset();
    let (tx, rx) = channel();
    ask_states(&st, &[0], 8, tx);
    match rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the stuck read answers at once") {
        OpMsg::Meta { line } => assert_eq!(line, r#"{"t":"tagstates","id":8,"applied":[],"mixed":[],"listing":4}"#),
        _ => panic!("tagstates answered on another message"),
    }
    assert_eq!(crate::backend::iomount::test_calls(), 1, "the stuck probe alone takes the bound, no reader thread");
}

#[test]
fn tagstates_answers_what_every_row_and_some_rows_carry() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsstates");
    let a = d.file("a.txt", "body");
    let b = d.file("b.txt", "body");
    crate::backend::tags::write_names(&a, &["Red".to_string(), "Blue".to_string(), "Custom".to_string()]).unwrap();
    crate::backend::tags::write_names(&b, &["Red".to_string()]).unwrap();
    assert_eq!(states_line(4, &[a.clone(), b.clone()]), r#"{"t":"tagstates","id":4,"applied":["Red"],"mixed":["Blue"]}"#);
    assert_eq!(states_line(5, &[]), r#"{"t":"tagstates","id":5,"applied":[],"mixed":[]}"#);
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    st.listing.push("b.txt", false);
    st.generation = 9;
    let (tx, rx) = channel();
    let line = r#"{"c":"tagstates","rows":[0,1],"id":6,"listing":9}"#;
    assert!(crate::backend::rowguard::refusal(line, 10).is_none(), "a read is never refused as stale");
    match crate::backend::proto::parse_request(line) {
        Request::TagStates { rows, id } => ask_states(&st, &rows, id, tx),
        _ => panic!("tagstates did not parse"),
    }
    match rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the read answers on the op channel") {
        OpMsg::Meta { line } => assert_eq!(line, r#"{"t":"tagstates","id":6,"applied":["Red"],"mixed":["Blue"],"listing":9}"#),
        _ => panic!("tagstates answered on another message"),
    }
    assert_eq!(crate::backend::tags::read(&b).unwrap(), vec!["Red"], "the read wrote nothing");
}
