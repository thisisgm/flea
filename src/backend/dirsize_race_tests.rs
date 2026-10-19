use super::{walk_counted, Progress};
use crate::backend::testdir::TestDir;
use std::cell::Cell;
use std::os::unix::fs::symlink;
use std::time::Duration;

#[test]
fn a_queued_child_swapped_for_a_link_is_never_followed_and_is_partial() {
    replaced_child(true);
}

#[test]
fn a_queued_child_swapped_for_a_different_directory_is_partial() {
    replaced_child(false);
}

fn replaced_child(link: bool) {
    let sandbox = TestDir::new("dirsize-replaced-child");
    let root = sandbox.dir("root");
    let child = sandbox.dir("root/child");
    sandbox.file("root/child/original", "original");
    let outside = sandbox.dir("outside");
    sandbox.file("outside/payload", &"x".repeat(1_000_000));
    let saved = sandbox.join("saved");
    let changed = Cell::new(false);
    let entering = |path: &std::path::Path| {
        if path == child && !changed.replace(true) {
            sandbox.assert_contains(&child);
            sandbox.assert_contains(&saved);
            std::fs::rename(&child, &saved).unwrap();
            if link { symlink(&outside, &child).unwrap(); }
            else {
                std::fs::create_dir(&child).unwrap();
                sandbox.file("root/child/replacement", &"x".repeat(1_000_000));
            }
        }
    };
    let tally = walk_counted(&root, &|| false, Some(Progress { every: Duration::ZERO, report: &mut |_| {}, entering: Some(&entering) }));
    assert!(changed.get(), "the test must replace a queued descendant");
    assert!(tally.partial, "a replaced child cannot be an exact total: {tally:?}");
    assert!(tally.bytes < 1_000_000, "the replacement's payload was read: {tally:?}");
}
