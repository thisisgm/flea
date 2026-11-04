use super::*;

#[test]
fn only_a_return_after_leaving_owes_a_reread() {
    let mut front = Front::default();
    assert!(
        !front.turn(false),
        "a window not yet shown has not returned"
    );
    assert!(
        !front.turn(true),
        "the launch's own activation re-reads nothing"
    );
    assert!(!front.turn(true), "a repeated activation re-reads nothing");
    assert!(!front.turn(false));
    assert!(
        front.turn(true),
        "coming back to the front owes the re-read"
    );
    assert!(!front.turn(true), "and only once");
}

#[test]
fn only_folders_inotify_cannot_hear_are_blind() {
    assert!(blind("fuse", ""));
    assert!(blind("nfs", "network"));
    assert!(blind("", "phone"));
    assert!(!blind("btrfs", ""));
    assert!(!blind("ext4", "usb"));
}

#[test]
fn a_return_reread_takes_the_watch_route_and_waits_while_the_rows_are_owned() {
    let mut watch = Watch::default();
    assert!(watch.changed("/mnt/remote", "/mnt/remote", 0));
    assert!(!watch.fire(WATCH_MS, true), "a held listing keeps the debt");
    assert!(watch.stale);
    watch.settle(WATCH_MS, false);
    assert!(watch.fire(2 * WATCH_MS, false));
    assert_eq!(watch.rereads, 1);
}
