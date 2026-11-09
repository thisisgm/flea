// A stored path decodes to its own bytes, and the bound and the NUL check meet the decoded path.
use super::*;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

fn stored(raw: &[u8]) -> Json {
    s(&Path::new(std::ffi::OsStr::from_bytes(raw)).exact_text())
}

#[test]
fn a_raw_path_reads_back_by_its_own_bytes_and_a_stray_nul_is_refused() {
    let raw = b"/x/caf\xe9.txt";
    assert_eq!(
        checked_path(&stored(raw)).unwrap().as_os_str().as_bytes(),
        raw
    );
    assert_eq!(
        checked_path(&s("/x/a\0zz")),
        None,
        "a NUL that spells no bytes is refused"
    );
}

#[test]
fn the_length_bound_counts_the_decoded_bytes_not_the_exact_spelling() {
    let at_bound = [b"/".as_slice(), &[0xff; MAX_PATH - 1]].concat();
    assert!(
        checked_path(&stored(&at_bound)).is_some(),
        "MAX_PATH raw bytes are a legal path"
    );
    let over = [b"/".as_slice(), &[0xff; MAX_PATH]].concat();
    assert_eq!(checked_path(&stored(&over)), None);
}
