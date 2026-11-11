// The arena keeps its shape: a UTF-8 row costs its name and one Span as before 0.4.2, a raw row adds its exact tail.
use super::*;
use crate::backend::listing::{Listing, Span};
use crate::backend::testdir::TestDir;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

// The Span before 0.4.2: two u32 fields and two flags, padded to 12 bytes.
const SPAN_BYTES_BEFORE: usize = 12;

fn arena(l: &Listing) -> usize {
    l.names.len() + l.spans.len() * std::mem::size_of::<Span>()
}

#[test]
fn a_utf8_row_costs_what_it_always_did_and_a_raw_row_adds_its_exact_tail() {
    let d = TestDir::new("scan-exact-utf8");
    let utf8: Vec<String> = (0..500)
        .map(|i| format!("report_final_draft_{i:04}.txt"))
        .collect();
    for name in &utf8 {
        d.file(name, "");
    }
    let (l, _) = scan(d.path().to_str().unwrap(), false).unwrap();
    assert_eq!(
        l.names.len(),
        utf8.iter().map(String::len).sum::<usize>(),
        "a UTF-8 name is stored byte for byte"
    );
    assert_eq!(
        (std::mem::size_of::<Span>(), l.spans.len()),
        (SPAN_BYTES_BEFORE, utf8.len()),
        "one Span per file, the same size as before"
    );

    let r = TestDir::new("scan-exact-raw");
    let raw: Vec<Vec<u8>> = (0..500)
        .map(|i| [format!("report_final_{i:04}").as_bytes(), b"\xff.txt"].concat())
        .collect();
    for name in &raw {
        std::fs::write(r.path().join(OsStr::from_bytes(name)), "").unwrap();
    }
    let (lr, _) = scan(r.path().to_str().unwrap(), false).unwrap();
    // The lossy spelling turns the one 0xff into three bytes; the tail is one NUL and two hex digits per raw byte.
    let lossy: usize = raw.iter().map(|name| name.len() + 2).sum();
    let tails: usize = raw.iter().map(|name| 1 + 2 * name.len()).sum();
    assert_eq!(
        lr.names.len(),
        lossy + tails,
        "a raw name costs its lossy spelling and its exact tail"
    );
    assert_eq!(lr.spans.len(), raw.len(), "a raw file still holds one Span");
    for i in 0..lr.len() {
        let exact = flea::bytename::text_path(lr.name(i));
        assert!(
            raw.iter()
                .any(|name| exact.as_os_str().as_bytes() == name.as_slice()),
            "row {i} names its own bytes"
        );
    }
    eprintln!(
        "ARENA bytes per row: utf8 {:.1} (names {:.1} + span {}), raw {:.1} (names {:.1}, lossy only {:.1})",
        arena(&l) as f64 / l.len() as f64,
        l.names.len() as f64 / l.len() as f64,
        std::mem::size_of::<Span>(),
        arena(&lr) as f64 / lr.len() as f64,
        lr.names.len() as f64 / lr.len() as f64,
        lossy as f64 / lr.len() as f64,
    );
}
