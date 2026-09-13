// The opsreq test module, out of line so src/backend/opsreq.rs stays inside the file budget.
use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::undo::Journal;
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc::{channel, Receiver};

// Drains an operation's channel to its terminal line, which is what every transfer case reads.
fn done_line(rx: Receiver<OpMsg>) -> (usize, usize, usize, bool, Entry) {
    let mut done = None;
    for msg in rx.iter() {
        if let OpMsg::TransferDone { ok, failed, skipped, cancelled, entry, .. } = msg {
            done = Some((ok, failed, skipped, cancelled, entry));
        }
    }
    done.expect("a terminal line")
}

#[test]
fn a_successful_item_line_carries_no_err_field_at_all() {
    let line = transferitem_line(12, 0, "a.txt", true, "");
    assert_eq!(line, r#"{"t":"transferitem","id":12,"index":0,"name":"a.txt","ok":true}"#);
    assert!(!line.contains("err"));
}

#[test]
fn a_dropbox_destination_replaced_before_worker_start_never_receives_the_source() {
    use crate::backend::menu_actions::Selected;
    let sandbox = TestDir::new("dropbox-worker-identity");
    let source = sandbox.file("source", "keep");
    let source_identity = ItemIdentity::record(&source.symlink_metadata().unwrap());
    let selected_source = Selected::inspect(source.to_str().unwrap()).unwrap();
    let destination = sandbox.dir("Dropbox");
    let captured = Selected::inspect(destination.to_str().unwrap()).unwrap();
    sandbox.assert_contains(&destination);
    std::fs::rename(&destination, sandbox.join("original-dropbox")).unwrap();
    sandbox.dir("Dropbox");
    let (tx, rx) = channel();
    sandbox.assert_contains(&source);
    sandbox.assert_contains(&destination);
    run_transfer_checked(1, true, vec![source.to_string_lossy().into()], destination.clone(),
        Arc::new(AtomicBool::new(false)), tx, Some(vec![selected_source.clone()]), Some(captured.clone()));
    let results: Vec<_> = rx.iter().collect();
    assert!(results.iter().any(|message| matches!(message, OpMsg::Item {ok: false, err, ..} if err.contains("Dropbox account folder changed"))));
    assert!(results.iter().any(|message| matches!(message, OpMsg::TransferDone {ok: 0, failed: 1, entry, retry, ..}
        if entry.steps.is_empty() && retry == &vec![(source.clone(), source_identity.clone())])));
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "keep");
    assert!(!destination.join("source").exists());
    sandbox.assert_contains(&source);
    std::fs::rename(&source, sandbox.join("original-source")).unwrap();
    sandbox.file("source", "replacement");
    let (tx, rx) = channel();
    sandbox.assert_contains(&source);
    sandbox.assert_contains(&destination);
    run_transfer_checked(2, true, vec![source.to_string_lossy().into()], destination.clone(),
        Arc::new(AtomicBool::new(false)), tx, Some(vec![selected_source]), Some(captured));
    assert!(rx.iter().any(|message| matches!(message, OpMsg::TransferDone {ok: 0, failed: 1, retry, ..} if retry.is_empty())));
    assert_eq!(std::fs::read_to_string(source).unwrap(), "replacement");
    assert!(!destination.join("source").exists());
}

#[test]
fn menu_workers_refuse_replacement_sources_before_helpers_or_mutations() {
    use crate::backend::menu_actions::{validate_sources, Selected};
    let d = TestDir::new("menu-workers-identity");
    let path = d.file("source.txt", "original");
    let outside_selection = d.file("other.txt", "other");
    let captured = vec![Selected::inspect(path.to_str().unwrap()).unwrap()];
    assert!(validate_sources(Some(&captured), std::slice::from_ref(&outside_selection)).is_err());
    assert!(d.path().is_absolute() && d.path().join(".flea-test-sandbox").is_file());
    assert!(path.is_absolute() && path.starts_with(d.path()));
    std::fs::rename(&path, d.join("original-moved")).unwrap();
    d.file("source.txt", "replacement");
    let (tx, rx) = channel();
    run_duplicate_checked(path.to_string_lossy().into(), tx, Some(captured.clone()));
    let OpMsg::Duplicated { ok, err, entry, .. } = rx.recv().unwrap() else { panic!("duplicate terminal result"); };
    assert!(!ok && err.contains("changed") && entry.steps.is_empty());
    assert!(path.is_absolute() && path.starts_with(d.path()));
    let (tx, rx) = channel();
    run_trash(vec![path.to_string_lossy().into()], tx, Some(captured.clone()));
    let results: Vec<_> = rx.iter().collect();
    assert!(results.iter().any(|message| matches!(message, OpMsg::Meta { line } if line.contains("changed"))));
    assert!(results.iter().any(|message| matches!(message, OpMsg::Trashed { ok: 0, failed: 1, entry } if entry.steps.is_empty())));
    let destination = d.join("output.zip");
    assert!(destination.is_absolute() && destination.starts_with(d.path()));
    let (tx, rx) = channel();
    crate::backend::archivereq::run_archive(1, true, vec![path.to_string_lossy().into()], "zip".into(), PathBuf::new(),
        destination.clone(), &crate::backend::archive::Formats::from_tools(false, false), tx, Some(captured.clone()));
    let OpMsg::Meta { line } = rx.recv().unwrap() else { panic!("archive terminal result"); };
    assert!(line.contains(r#""ok":false"#) && line.contains("changed"));
    let (tx, rx) = channel();
    crate::backend::archivereq::run_convert(2, 0, path.clone(), destination.clone(), false, tx, Some(captured));
    let OpMsg::Meta { line } = rx.recv().unwrap() else { panic!("convert terminal result"); };
    assert!(line.contains(r#""ok":false"#) && line.contains("changed"));
    assert!(!destination.exists());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    assert_eq!(std::fs::read_to_string(outside_selection).unwrap(), "other");
}

#[test]
fn a_failed_item_line_carries_its_reason_escaped() {
    let line = transferitem_line(12, 1, "say \"hi\".txt", false, "permission denied");
    assert!(line.contains(r#""ok":false"#));
    assert!(line.contains(r#""err":"permission denied""#));
    assert!(line.contains(r#"say \"hi\".txt"#), "a name is escaped like every other string on this wire");
}

#[test]
fn every_operation_line_matches_the_shape_the_operations_design_names() {
    assert_eq!(transferstarted_line(12, 2, false), r#"{"t":"transferstarted","id":12,"n":2,"moving":false}"#);
    assert_eq!(transferstarted_line(12, 2, true), r#"{"t":"transferstarted","id":12,"n":2,"moving":true}"#);
    assert_eq!(
        transferprogress_line(12, 0, "a.txt", 40000000, 120000000),
        r#"{"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000}"#
    );
    assert_eq!(
        transferdone_line(12, 1, 1, 0, false, &[]),
        r#"{"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false,"retryPaths":[]}"#
    );
    assert_eq!(trashed_line(1, 0), r#"{"t":"trashed","ok":1,"failed":0}"#);
    assert_eq!(renamed_line(true, "/home/gm/new.txt"), r#"{"t":"renamed","ok":true,"path":"/home/gm/new.txt"}"#);
    assert_eq!(
        duplicated_line(true, "/home/gm/photo copy.jpg"),
        r#"{"t":"duplicated","ok":true,"path":"/home/gm/photo copy.jpg"}"#
    );
    assert_eq!(made_line(true, "/home/gm/New Folder"), r#"{"t":"made","ok":true,"path":"/home/gm/New Folder"}"#);
    assert_eq!(undone_line("move", true), r#"{"t":"undone","op":"move","ok":true}"#);
}

#[test]
fn a_destination_that_is_not_an_existing_directory_is_refused_before_any_item_is_touched() {
    let d = TestDir::new("dest");
    assert!(usable_dest(&d.path().to_string_lossy()).is_ok());
    let file = d.file("not-a-dir.txt", "body");
    assert_eq!(
        usable_dest(&file.to_string_lossy()).unwrap_err().msg,
        "the destination is not a directory"
    );
    assert_eq!(
        usable_dest("relative/path").unwrap_err().msg,
        "a destination must be an absolute path",
        "a relative destination is never resolved here"
    );
    let missing = d.join("missing");
    let error = usable_dest(&missing.to_string_lossy()).unwrap_err();
    assert_eq!(error.where_, "transfer");
    assert_eq!(error.path, missing.to_string_lossy());
    assert_eq!(error.msg, "file or folder not found");
    assert!(!missing.exists(), "Flea does not create the destination");
}

#[test]
fn a_folder_is_refused_into_itself_and_into_its_own_subtree_while_a_lookalike_sibling_lands() {
    let d = TestDir::new("intoitself");
    let src = d.dir("x");
    d.file("x/a.txt", "body");
    let deep = d.dir("x/deep");
    let sibling = d.dir("x2");
    let (tx, rx) = channel();
    let paths = vec![src.to_string_lossy().to_string()];
    run_transfer(1, false, paths.clone(), src.clone(), Arc::new(AtomicBool::new(false)), tx);
    let (ok, failed, _, _, entry) = done_line(rx);
    assert_eq!((ok, failed), (0, 1), "a folder into itself is one refused item");
    assert!(entry.steps.is_empty(), "and nothing was created");
    assert!(!src.join("x").exists());
    let (tx, rx) = channel();
    run_transfer(2, true, paths.clone(), deep.clone(), Arc::new(AtomicBool::new(false)), tx);
    assert_eq!(done_line(rx).1, 1, "a move into its own subtree is refused the same way");
    assert!(src.join("a.txt").exists(), "and the source is untouched");
    let (tx, rx) = channel();
    run_transfer(3, false, paths, sibling.clone(), Arc::new(AtomicBool::new(false)), tx);
    assert_eq!(done_line(rx).0, 1, "x2 is not inside x, so the copy lands");
    assert_eq!(std::fs::read_to_string(sibling.join("x/a.txt")).unwrap(), "body");
}

// The refused item's own sentence, so a test pins the branch and not only the count.
fn refusal(rx: Receiver<OpMsg>) -> (usize, usize, Vec<Step>, String) {
    let mut err = String::new();
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: false, err: e, .. } => err = e,
            OpMsg::TransferDone { ok, failed, entry, .. } => done = Some((ok, failed, entry.steps)),
            _ => {}
        }
    }
    let (ok, failed, steps) = done.expect("a terminal line");
    (ok, failed, steps, err)
}

#[test]
fn a_file_dropped_into_its_own_folder_is_refused_with_its_bytes_intact() {
    let d = TestDir::new("alreadythere");
    let file = d.file("a.txt", "body");
    let (tx, rx) = channel();
    run_transfer(1, false, vec![file.to_string_lossy().to_string()], d.path().to_path_buf(), Arc::new(AtomicBool::new(false)), tx);
    let (ok, failed, steps, err) = refusal(rx);
    assert_eq!((ok, failed), (0, 1), "a copy onto itself is one refused item");
    assert_eq!(err, ALREADY_THERE, "and it is this refusal, not the folder-into-itself one");
    assert!(steps.is_empty(), "and nothing was journalled");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "body", "the bytes were never opened for writing");
    let (tx, rx) = channel();
    run_transfer(2, true, vec![file.to_string_lossy().to_string()], d.path().to_path_buf(), Arc::new(AtomicBool::new(false)), tx);
    assert_eq!(refusal(rx).3, ALREADY_THERE, "a move onto itself is refused the same way");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "body");
}

#[test]
fn the_same_folder_reached_through_a_symlink_is_still_itself() {
    let d = TestDir::new("symlinkedself");
    let real = d.dir("real");
    let file = d.file("real/a.txt", "body");
    d.dir("real/deep");
    let link = d.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let (tx, rx) = channel();
    run_transfer(1, false, vec![file.to_string_lossy().to_string()], link.clone(), Arc::new(AtomicBool::new(false)), tx);
    let (_, failed, _, err) = refusal(rx);
    assert_eq!((failed, err.as_str()), (1, ALREADY_THERE), "a file into its own folder through a link is a copy onto itself");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "body");
    let (tx, rx) = channel();
    run_transfer(2, false, vec![real.to_string_lossy().to_string()], link.join("deep"), Arc::new(AtomicBool::new(false)), tx);
    assert_eq!(refusal(rx).3, INTO_ITSELF, "and a folder into its own subtree through a link is still into itself");
}

// Review of 0.1.6: the guard canonicalised a link source, so a link to a folder could not land inside that folder.
#[test]
fn a_link_to_a_folder_lands_inside_that_folder_as_a_link_while_the_folder_itself_is_refused() {
    let d = TestDir::new("linksource");
    let real = d.dir("real");
    d.file("real/a.txt", "body");
    let out = d.dir("real/out");
    let link = d.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let (tx, rx) = channel();
    run_transfer(1, false, vec![link.to_string_lossy().to_string()], out.clone(), Arc::new(AtomicBool::new(false)), tx);
    let (ok, failed, _, _, entry) = done_line(rx);
    assert_eq!((ok, failed), (1, 0), "the link is the item, and a link holds nothing");
    let landed = out.join("link");
    assert!(landed.symlink_metadata().unwrap().file_type().is_symlink(), "what landed is a link, not a copy of the tree");
    assert_eq!(std::fs::read_link(&landed).unwrap(), real, "and it still points where the source pointed");
    assert_eq!(entry.steps, vec![undo::copied(&link, &landed, ItemIdentity::inspect(&link).unwrap()).unwrap()]);
    let (tx, rx) = channel();
    run_transfer(2, false, vec![real.to_string_lossy().to_string()], out.clone(), Arc::new(AtomicBool::new(false)), tx);
    assert_eq!(refusal(rx).3, INTO_ITSELF, "the real folder into its own subtree is still refused");
    let (tx, rx) = channel();
    run_transfer(3, true, vec![link.to_string_lossy().to_string()], d.dir("elsewhere"), Arc::new(AtomicBool::new(false)), tx);
    assert_eq!(done_line(rx).0, 1, "a move of the link is a rename of the link");
    assert!(!link.exists() && d.join("elsewhere/link").symlink_metadata().unwrap().file_type().is_symlink());
    assert!(real.join("a.txt").exists(), "and the target was never touched");
}

#[test]
fn a_copy_transfer_records_only_what_it_created_and_leaves_the_sources() {
    let d = TestDir::new("transfercopy");
    let src = d.file("a.txt", "body");
    let dest = d.dir("out");
    let (tx, rx) = channel();
    run_transfer(1, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    assert!(src.exists(), "a copy leaves its source");
    assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "body");
    let (ok, failed, _, _, entry) = done_line(rx);
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(entry.op, "copy");
    assert_eq!(entry.steps, vec![undo::copied(&src, &dest.join("a.txt"), ItemIdentity::inspect(&src).unwrap()).unwrap()]);
}

#[test]
fn a_move_transfer_records_where_each_item_came_from() {
    let d = TestDir::new("transfermove");
    let src = d.file("b.txt", "body");
    let dest = d.dir("out");
    let (tx, rx) = channel();
    run_transfer(2, true, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    assert!(!src.exists(), "a move leaves nothing at the source");
    let (_, _, _, _, entry) = done_line(rx);
    assert_eq!(entry.op, "move");
    assert!(matches!(&entry.steps[..], [Step::Moved { from, to, after, .. }]
        if from == &src && to == &dest.join("b.txt") && after == &ItemIdentity::inspect(to).unwrap()));
}

#[test]
fn one_failing_item_is_data_and_the_batch_carries_on() {
    let d = TestDir::new("transferpartial");
    let good = d.file("good.txt", "body");
    let missing = d.join("never-existed.txt");
    let dest = d.dir("out");
    let (tx, rx) = channel();
    run_transfer(
        3,
        false,
        vec![missing.to_string_lossy().to_string(), good.to_string_lossy().to_string()],
        dest.clone(),
        Arc::new(AtomicBool::new(false)),
        tx,
    );
    assert!(dest.join("good.txt").exists(), "the item after the failure still ran");
    let mut counts = None;
    let mut errs = Vec::new();
    for msg in rx.iter() {
        match msg {
            OpMsg::TransferDone { ok, failed, .. } => counts = Some((ok, failed)),
            OpMsg::Item { id, index, name, ok: false, err } => errs.push((id, index, name, err)),
            _ => {}
        }
    }
    assert_eq!(counts, Some((1, 1)));
    assert_eq!(errs, vec![(3, 0, "never-existed.txt".into(), "file or folder not found".into())],
        "the plain failure cause retains its operation and item identity");
}

// A file with no permission bits answers EACCES to open(2) for every uid but root, so it forces
// the failure a permission error would, in whichever order read_dir yields; what was copied stays.
#[test]
fn a_copy_that_fails_short_of_a_cancel_records_the_partial_tree_and_undo_removes_it() {
    let d = TestDir::new("transferpartialtree");
    let src = d.dir("tree");
    std::fs::write(src.join("good.txt"), "body").unwrap();
    let shut = src.join("shut.txt");
    std::fs::write(&shut, "body").unwrap();
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    let dest = d.dir("out");
    let (tx, rx) = channel();
    run_transfer(5, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    let (ok, failed, _, cancelled, entry) = done_line(rx);
    assert_eq!((ok, failed, cancelled), (0, 1, false));
    let partial = dest.join("tree");
    assert!(partial.is_dir(), "a failure that is not a cancel leaves what it copied");
    assert_eq!(entry.steps, vec![undo::copied(&src, &partial, ItemIdentity::inspect(&src).unwrap()).unwrap()], "the partial tree is journaled");
    let mut j = Journal::new();
    j.push(entry);
    assert_eq!(j.undo().expect("undo"), "copy");
    assert!(!partial.exists(), "undo removed the partial tree");
    assert!(src.join("good.txt").exists(), "and left the source alone");
}

#[test]
fn a_copy_refused_at_an_existing_destination_records_nothing_to_undo() {
    let d = TestDir::new("transferclobber");
    let src = d.file("a.txt", "new");
    let dest = d.dir("out");
    std::fs::write(dest.join("a.txt"), "already here").unwrap();
    let (tx, rx) = channel();
    run_transfer(6, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    let (_, failed, _, _, entry) = done_line(rx);
    assert_eq!(failed, 1);
    assert!(entry.steps.is_empty(), "undo must never remove what the user already had");
    assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "already here");
}

#[test]
fn a_cancelled_transfer_skips_the_rest_and_says_so() {
    let d = TestDir::new("transfercancel");
    let a = d.file("a.txt", "one");
    let b = d.file("b.txt", "two");
    let dest = d.dir("out");
    let (tx, rx) = channel();
    run_transfer(
        4,
        false,
        vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()],
        dest.clone(),
        Arc::new(AtomicBool::new(true)),
        tx,
    );
    let (ok, failed, skipped, cancelled, _) = done_line(rx);
    assert_eq!((ok, failed, skipped, cancelled), (0, 0, 2, true));
    assert!(!dest.join("a.txt").exists(), "a cancel before the first item copies nothing");
}

#[test]
fn failed_transfer_retry_retains_only_original_sources_after_permission_repair() {
    let d = TestDir::new("transfer-retry");
    let failed_source = d.file("failed.txt", "original");
    let good = d.file("good.txt", "copied");
    let dest = d.dir("out");
    let collision = d.file("out/failed.txt", "occupied");
    let (tx, rx) = channel();
    run_transfer(9, false, vec![failed_source.to_string_lossy().into(), good.to_string_lossy().into()],
        dest.clone(), Arc::new(AtomicBool::new(false)), tx);
    let retry = rx.into_iter().find_map(|message| match message {
        OpMsg::TransferDone { ok, failed, skipped, cancelled, retry, .. } => {
            assert_eq!((ok, failed, skipped, cancelled), (1, 1, 0, false));
            Some(retry)
        }
        _ => None,
    }).expect("transfer terminal event");
    assert_eq!(retry.len(), 1);
    assert_eq!(retry[0].0, failed_source);
    std::fs::set_permissions(&failed_source, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut matches = vec![(failed_source.to_str().unwrap(), 3), (collision.to_str().unwrap(), 4), (good.to_str().unwrap(), 5)];
    retain_retry(&retry, &mut matches);
    assert_eq!(matches, vec![(failed_source.to_str().unwrap(), 3)]);
    let line = transferdone_line(9, 1, 1, 0, false, &retry);
    assert_eq!(crate::json::field_str_array(&line, "retryPaths"), [failed_source.to_string_lossy().into_owned()]);
    assert_eq!(std::fs::read_to_string(&collision).unwrap(), "occupied");
    assert_eq!(std::fs::read_to_string(dest.join("good.txt")).unwrap(), "copied");

    assert!(failed_source.is_absolute() && failed_source.starts_with(d.path()) && d.path().join(".flea-test-sandbox").is_file());
    std::fs::rename(&failed_source, d.join("original-moved.txt")).unwrap();
    d.file("failed.txt", "replacement");
    let mut replaced = vec![(failed_source.to_str().unwrap(), 3)];
    retain_retry(&retry, &mut replaced);
    assert!(replaced.is_empty(), "a replacement at the failed name is never selected for retry");
    assert_eq!(std::fs::read_to_string(&failed_source).unwrap(), "replacement");
}

#[test]
fn retry_preserves_a_symlink_identity_without_following_its_target() {
    let d = TestDir::new("transfer-retry-link");
    let target = d.file("target.txt", "first target");
    let link = d.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let retry = vec![(link.clone(), ItemIdentity::inspect(&link).unwrap())];
    std::fs::write(&target, "changed target").unwrap();
    let mut matches = vec![(link.to_str().unwrap(), 2)];
    retain_retry(&retry, &mut matches);
    assert_eq!(matches.len(), 1);
    assert!(link.is_absolute() && link.starts_with(d.path()) && d.path().join(".flea-test-sandbox").is_file());
    std::fs::rename(&link, d.join("original-link")).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    retain_retry(&retry, &mut matches);
    assert!(matches.is_empty());
}
