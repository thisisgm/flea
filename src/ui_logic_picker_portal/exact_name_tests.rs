// A picked name that is not UTF-8 leaves as a URI of its own bytes, beside tests.rs at its ceiling.
use super::*;

#[test]
fn a_raw_picked_name_leaves_as_a_uri_of_its_own_bytes() {
    assert_eq!(
        file_uri("/caf\u{fffd}\u{0}636166e9"),
        "file:///caf%E9",
        "a raw name escapes its own bytes"
    );
}
