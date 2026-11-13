// A merge onto a drive that cannot hold links names each refused link on its done message.
use super::*;

// Clears the forced linkless drive on the way out, a failed assert included.
struct Linkless;
impl Drop for Linkless {
    fn drop(&mut self) {
        crate::backend::unheld::test_force_linkless(false);
    }
}

// Sample input: from/trip/sub holds a.txt and a link, merged into a to/trip without sub on a linkless drive.
#[test]
fn a_merge_onto_a_linkless_drive_names_the_refused_link_on_merge_done() {
    let d = TestDir::new("merge-refused-link");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    let sub = d.dir("from/trip/sub");
    d.file("from/trip/sub/a.txt", "a");
    std::os::unix::fs::symlink("a.txt", sub.join("link")).unwrap();
    d.dir("to/trip");
    let policy = merged_policy(7, &[&album], &to, "keep");
    let (tx, rx) = channel();
    crate::backend::unheld::test_force_linkless(true);
    let _reset = Linkless;
    run_transfer_checked(1, false, owned(&[&album]), to.clone(), AtomicBool::new(false).into(), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: policy });
    let refused = rx.iter().find_map(|msg| match msg {
        OpMsg::MergeDone { refused, .. } => Some(refused),
        _ => None,
    });
    assert_eq!(refused, Some(vec![("trip/sub/link".to_string(), "this drive cannot hold links".to_string())]), "the link is named under the destination");
    assert_eq!(text(&to.join("trip/sub/a.txt")), "a", "the file beside the link lands");
    assert!(to.join("trip/sub/link").symlink_metadata().is_err(), "the link is not copied");
}
