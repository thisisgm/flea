// Copy As text forms of a name that is not UTF-8: lossy as text, its own bytes as a URI and a shell word.
use crate::tui::parity17;

#[test]
fn a_raw_named_copy_reads_lossy_as_text_and_exact_as_a_uri() {
    use flea::bytename::ExactText as _;
    use std::os::unix::ffi::OsStrExt;
    let raw = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/x/r\xff.txt"))
        .exact_text()
        .into_owned();
    let copy = |form: &str| {
        parity17::copy_text(
            std::slice::from_ref(&raw),
            form,
            std::path::Path::new("/empty"),
        )
    };
    assert_eq!(copy("path"), "/x/r\u{fffd}.txt");
    assert_eq!(copy("filename"), "r\u{fffd}.txt");
    assert_eq!(copy("url"), "file:///x/r%FF.txt");
    assert_eq!(
        copy("quoted"),
        "$'/x/r\\xff.txt'",
        "the shell word reproduces the bytes"
    );
    let relative = parity17::copy_text(
        std::slice::from_ref(&raw),
        "relative",
        std::path::Path::new("/x"),
    );
    assert_eq!(relative, "r\u{fffd}.txt");
}
