use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn popping_the_base_returns_an_error_and_keeps_it_held() {
    let d = TestDir::new("chain-base");
    let mut chain = Chain::new(open_dir(d.path()).unwrap());
    let id = identity_of(chain.top()).unwrap();
    assert_eq!(chain.pop().unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert_eq!(chain.depth(), 0);
    assert_eq!(identity_of(chain.top()).unwrap(), id);
}

#[test]
fn a_failed_parent_reopen_keeps_the_child_and_can_be_retried() {
    let d = TestDir::new("chain-reopen");
    let root = d.dir("tree");
    let mut chain = Chain::new(open_dir(&root).unwrap());
    for _ in 0..=HELD_LEVELS {
        std::fs::create_dir(held_path(chain.top()).join("a")).unwrap();
        chain.push(OsStr::new("a")).unwrap();
    }
    while chain.depth() > 2 {
        chain.pop().unwrap();
    }
    let id = identity_of(chain.top()).unwrap();
    let moved = d.join("moved");
    std::fs::rename(root.join("a/a"), &moved).unwrap();
    assert!(chain.pop().is_err());
    assert_eq!(chain.depth(), 2, "failed climb keeps its place");
    assert_eq!(identity_of(chain.top()).unwrap(), id);
    std::fs::rename(&moved, root.join("a/a")).unwrap();
    chain.pop().unwrap();
    assert_eq!(chain.depth(), 1);
}
