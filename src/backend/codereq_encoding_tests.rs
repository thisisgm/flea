use super::*;

// S1A-8: Latin-1 text reads as its characters, UTF-16 by its mark is text and not binary, and a UTF-8 mark is not drawn.
#[test]
fn legacy_and_utf16_text_read_as_their_characters() {
    let d = TestDir::new("codereqencoding");
    let write = |name: &str, bytes: &[u8]| {
        let path = d.join(name);
        std::fs::write(&path, bytes).expect("the fixture is written");
        path
    };
    let latin1 = answered(
        "codereq-latin1",
        &file_ask(&write("f-latin1.txt", b"caf\xe9 na\xefve\n"), ""),
        None,
    );
    assert_eq!(
        (state(&latin1), firsts(&latin1)),
        (
            "plain".to_string(),
            vec![(0, "caf\u{e9} na\u{ef}ve".to_string())]
        )
    );
    let utf16: Vec<u8> = "\u{feff}hello utf16\nline two\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let wide = answered(
        "codereq-utf16",
        &file_ask(&write("f-utf16.txt", &utf16), ""),
        None,
    );
    assert_eq!(
        (state(&wide), total(&wide)),
        ("plain".to_string(), 2),
        "{wide}"
    );
    assert_eq!(firsts(&wide)[0], (0, "hello utf16".to_string()));
    let bom = answered(
        "codereq-bom",
        &file_ask(&write("f-bom.txt", b"\xef\xbb\xbfmarked\n"), ""),
        None,
    );
    assert_eq!(firsts(&bom)[0], (0, "marked".to_string()));
    // A NUL with no UTF-16 mark is still binary.
    let blob = answered(
        "codereq-blob",
        &file_ask(&write("blob.bin", b"\x00\x01\xe9"), ""),
        None,
    );
    assert_eq!(state(&blob), "binary");
}

// A UTF-16 file past the large mark stays large, though its decoded text is half its bytes.
#[test]
fn a_large_utf16_file_is_still_large() {
    let d = TestDir::new("codereqencodinglarge");
    let mut wide: Vec<u8> = vec![0xff, 0xfe];
    wide.extend(
        "a\n"
            .repeat(LARGE_BYTES / 4 + 1)
            .encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    let path = d.join("f-wide.txt");
    std::fs::write(&path, &wide).expect("the fixture is written");
    assert_eq!(
        state(&answered("codereq-utf16-large", &file_ask(&path, ""), None)),
        "large"
    );
}
