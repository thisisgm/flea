use super::{open_same, stat};
use crate::backend::testdir::TestDir;
use std::os::unix::fs::symlink;

#[test]
fn open_same_refuses_a_replaced_directory_before_descent() {
    let sandbox = TestDir::new("dirsize-open-identity");
    let child = sandbox.dir("child");
    let expected = stat::at(&child).unwrap().identity();
    let saved = sandbox.join("saved");
    sandbox.assert_contains(&child);
    sandbox.assert_contains(&saved);
    std::fs::rename(&child, &saved).unwrap();
    std::fs::create_dir(&child).unwrap();
    assert!(
        open_same(&child, expected).is_none(),
        "a queued identity must refuse a replacement directory"
    );
    assert!(
        open_same(&saved, expected).is_some(),
        "the original directory must still open"
    );
}

#[test]
fn open_same_refuses_a_link_even_when_its_target_has_the_expected_identity() {
    let sandbox = TestDir::new("dirsize-open-nofollow");
    let child = sandbox.dir("child");
    let expected = stat::at(&child).unwrap().identity();
    let saved = sandbox.join("saved");
    sandbox.assert_contains(&child);
    sandbox.assert_contains(&saved);
    std::fs::rename(&child, &saved).unwrap();
    symlink(&saved, &child).unwrap();
    assert!(
        open_same(&child, expected).is_none(),
        "O_NOFOLLOW must refuse even a link to the original directory"
    );
    assert!(
        open_same(&saved, expected).is_some(),
        "the original directory must still open"
    );
}
