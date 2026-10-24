use super::*;

#[test]
fn native31_real_fsinfo_wire_updates_mounted_footer_and_size_class() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut read) = open(&mut cx);
    let startup = requests(&mut read);
    assert!(!startup.is_empty());
    assert!(startup
        .iter()
        .all(|r| r.get("c").and_then(Json::as_str) == Some("fsinfo")));
    answer(&mut cx, handle, 0, "/files", "btrfs", "network");
    disk(&mut cx, handle, "btrfs · 442.0 GB free");
    advance(&mut cx);
    assert!(
        requests(&mut read).is_empty(),
        "network class started a folder walk"
    );
    answer(&mut cx, handle, 0, "/files", "btrfs", "");
    advance(&mut cx);
    let sent = requests(&mut read);
    assert_eq!(
        sent.len(),
        1,
        "real fsinfo class did not reach the size planner: {sent:?}"
    );
    assert_eq!(sent[0].get("c").and_then(Json::as_str), Some("dirsize"));
    assert_eq!(sent[0].get("listing").and_then(Json::as_f64), Some(3.));
    assert_eq!(
        sent[0].get("rows").and_then(Json::as_array).unwrap().len(),
        2
    );
    edit(&mut cx, handle, |shell, _, _| {
        assert!(shell.surfaces[0].filesystem.answered);
        assert!(
            shell.surfaces[1].filesystem.facts.is_none(),
            "left answer crossed into right pane"
        );
    });
    answer(&mut cx, handle, 1, "/right", "xfs", "network");
    disk(&mut cx, handle, "btrfs · 442.0 GB free");
    edit(&mut cx, handle, |shell, _, cx| shell.focus_pane(1, cx));
    frame(&mut cx, handle);
    disk(&mut cx, handle, "xfs · 442.0 GB free");
}

#[test]
fn native31_wire_keeps_known_and_empty_pending_answers_scoped_to_the_committed_path() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _read) = open(&mut cx);
    answer(&mut cx, handle, 0, "/files", "btrfs", "network");
    for (old, new, listing, retained) in [
        ("/files", "/next", 4, "btrfs · 442.0 GB free"),
        ("/next", "/last", 5, ""),
    ] {
        edit(&mut cx, handle, |shell, window, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin(new.into(), Default::default(), pane.now()));
            commit(shell, 0, new, listing, window, cx);
        });
        frame(&mut cx, handle);
        disk(&mut cx, handle, retained);
        answer(&mut cx, handle, 0, old, "obsolete", "");
        disk(&mut cx, handle, retained);
        edit(&mut cx, handle, |shell, _, _| {
            assert!(
                !shell.surfaces[0].filesystem.answered,
                "obsolete reply answered the new directory"
            );
        });
        answer(&mut cx, handle, 0, new, "", "network");
        edit(&mut cx, handle, |shell, _, _| {
            assert!(shell.surfaces[0].filesystem.answered)
        });
    }
    answer(&mut cx, handle, 0, "/last", "ext4", "network");
    disk(&mut cx, handle, "ext4 · 442.0 GB free");
}

#[test]
fn native31_busy_size_class_guard_does_not_discard_matching_footer_facts() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, mut read) = open(&mut cx);
    let _startup = requests(&mut read);
    answer(&mut cx, handle, 0, "/files", "btrfs", "network");
    advance(&mut cx);
    assert!(requests(&mut read).is_empty());
    edit(&mut cx, handle, |shell, _, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/files".into(), Default::default(), pane.now()));
        assert!(pane.busy());
    });
    answer(&mut cx, handle, 0, "/files", "ext4", "");
    disk(&mut cx, handle, "ext4 · 442.0 GB free");
    edit(&mut cx, handle, |shell, window, cx| {
        commit(shell, 0, "/files", 4, window, cx)
    });
    frame(&mut cx, handle);
    advance(&mut cx);
    let sent = requests(&mut read);
    assert!(
        sent.iter()
            .all(|r| r.get("c").and_then(Json::as_str) == Some("fsinfo")),
        "busy class replaced the retained network refusal: {sent:?}"
    );
    answer(&mut cx, handle, 0, "/files", "ext4", "");
    advance(&mut cx);
    let sent = requests(&mut read);
    assert_eq!(sent.len(), 1, "ready class was not accepted: {sent:?}");
    assert_eq!(sent[0].get("c").and_then(Json::as_str), Some("dirsize"));
}
