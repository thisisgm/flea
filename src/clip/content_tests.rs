use super::*;
use crate::backend::fifotest::{mkfifo, within};
use crate::backend::testdir::TestDir;

#[test]
fn one_regular_file_is_offered_as_its_own_type_and_bytes() {
    let d = TestDir::new("clip-content-one");
    let png = d.file("shot.png", "\u{89}PNG fake");
    let (mime, bytes) = read_as(&png, "image/png".to_string()).expect("a png is offered");
    assert_eq!(mime, "image/png");
    assert_eq!(bytes, std::fs::read(&png).unwrap());
}

#[test]
fn a_name_no_glob_claims_is_offered_as_octet_stream() {
    let d = TestDir::new("clip-content-bin");
    let bin = d.file("firmware-no-glob-xyzzy", "\0\x01\x02");
    let path = bin.to_string_lossy().to_string();
    let (mime, bytes) = read(&[path]).expect("an unknown file is still its bytes");
    assert_eq!(mime, UNKNOWN);
    assert_eq!(bytes, b"\0\x01\x02");
}

#[test]
fn several_paths_carry_no_bytes() {
    let d = TestDir::new("clip-content-many");
    let a = d.file("a.png", "a").to_string_lossy().to_string();
    let b = d.file("b.png", "b").to_string_lossy().to_string();
    assert!(read(&[a, b]).is_none(), "no standard type carries several files' bytes");
}

#[test]
fn a_folder_or_a_fifo_or_a_missing_file_carries_no_bytes() {
    let d = TestDir::new("clip-content-kinds");
    assert!(read_as(&d.dir("sub"), UNKNOWN.to_string()).is_none());
    assert!(read_as(&d.join("never-existed"), UNKNOWN.to_string()).is_none());
    let fifo = d.join("pipe");
    mkfifo(&fifo);
    assert!(within("read_as on a fifo", move || read_as(&fifo, UNKNOWN.to_string())).is_none());
}

#[test]
fn a_type_the_path_list_already_answers_is_not_replaced() {
    let d = TestDir::new("clip-content-plain");
    let txt = d.file("notes.txt", "hello");
    assert!(read_as(&txt, "text/plain".to_string()).is_none(), "text/plain stays the path list");
    assert!(read_as(&txt, "text/uri-list".to_string()).is_none());
}

#[test]
fn a_file_past_the_cap_carries_no_bytes() {
    let d = TestDir::new("clip-content-big");
    let big = d.join("big.bin");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(MAX_BYTES + 1).unwrap();
    assert!(read_as(&big, UNKNOWN.to_string()).is_none());
    f.set_len(MAX_BYTES).unwrap();
    assert_eq!(read_as(&big, UNKNOWN.to_string()).expect("at the cap is fine").1.len() as u64, MAX_BYTES);
}
