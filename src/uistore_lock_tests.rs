use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn blocking_and_nonblocking_wrappers_share_the_file_lock() {
    let root = TestDir::new("uistore-lock-wrappers");
    let path = root.join("state.lock");
    let first = fs::File::create(&path).unwrap();
    let second = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    println!("lock probe path={}", path.display());
    lock_exclusive(&first).unwrap();
    assert!(!try_lock_exclusive(&second).unwrap());
    unlock(&first).unwrap();
    assert!(try_lock_exclusive(&second).unwrap());
    unlock(&second).unwrap();
}
