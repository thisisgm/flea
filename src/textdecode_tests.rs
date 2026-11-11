use super::*;

#[test]
fn a_utf8_byte_order_mark_is_dropped() {
    assert_eq!(decode(b"\xef\xbb\xbf# bom markdown\n"), "# bom markdown\n");
    assert!(matches!(decode(b"plain\n"), Cow::Borrowed("plain\n")));
}

#[test]
fn utf16_is_read_by_its_byte_order_mark() {
    let little: Vec<u8> = "\u{feff}hello utf16\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let big: Vec<u8> = "\u{feff}h\u{e9}\n"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert_eq!(decode(&little), "hello utf16\n");
    assert_eq!(decode(&big), "h\u{e9}\n");
    assert!(looks_text(&little, false));
}

#[test]
fn text_that_is_not_utf8_reads_as_windows_1252() {
    assert_eq!(decode(b"caf\xe9 na\xefve\n"), "caf\u{e9} na\u{ef}ve\n");
    assert_eq!(
        decode(b"\x93quoted\x94 \x80 \x81"),
        "\u{201c}quoted\u{201d} \u{20ac} \u{81}"
    );
    assert!(looks_text(b"caf\xe9\n", false));
    // A high byte that ends the file counts as a stray byte too, so no tie sends it to UTF-8.
    assert_eq!(decode(b"caf\xe9"), "caf\u{e9}");
}

#[test]
fn bytes_with_a_nul_stay_binary() {
    let decoded = decode(b"\x00\x01\xff binary");
    assert!(
        decoded.contains('\0'),
        "the NUL survives for the binary check"
    );
    assert!(!looks_text(b"\x00\x01\x02", false));
    assert!(!looks_text(b"\x01\x02\x03\xff", false));
    // A UTF-16 mark ahead of NUL pairs is still not text.
    assert!(!looks_text(b"\xff\xfe\x00\x00\x00\x00", false));
}

#[test]
fn a_character_the_cut_went_through_is_still_text() {
    // The control byte fails the single-byte rule, so only the cut can make this head text.
    let head = "\u{1}caf\u{e9}".as_bytes();
    assert!(looks_text(&head[..head.len() - 1], true));
    assert!(!looks_text(&head[..head.len() - 1], false));
}

#[test]
fn one_stray_byte_in_utf8_text_stays_one_replacement_character() {
    assert_eq!(
        decode(b"caf\xc3\xa9 na\xc3\xafve \xe9\n"),
        "caf\u{e9} na\u{ef}ve \u{fffd}\n"
    );
    assert_eq!(
        decode(b"\xe9 caf\xc3\xa9 \xc3\xa0"),
        "\u{fffd} caf\u{e9} \u{e0}"
    );
    // One valid character against one stray byte is a tie, and a tie is UTF-8.
    assert_eq!(decode(b"caf\xc3\xa9 \xe9\n"), "caf\u{e9} \u{fffd}\n");
    // Only ASCII around the stray bytes is still single-byte text.
    assert_eq!(decode(b"caf\xe9 \x93q\x94"), "caf\u{e9} \u{201c}q\u{201d}");
}

#[test]
fn windows_1252_bytes_that_pair_up_by_chance_stay_windows_1252() {
    // C9 A0 is valid UTF-8 (U+0260) but here an E-acute and a no-break space.
    assert_eq!(
        decode(b"\xc9T\xc9\xa0: caf\xe9 na\xefve"),
        "\u{c9}T\u{c9}\u{a0}: caf\u{e9} na\u{ef}ve"
    );
}
