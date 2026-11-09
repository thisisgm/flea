// Copy names for long originals (soak S4-8).
use crate::backend::ops::{copy_name, duplicate};
use crate::backend::testdir::TestDir;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

// The longest name ext4 and btrfs take, in bytes.
const NAME_MAX: usize = 255;

// S4-8: Duplicate and Keep refused names of 251 bytes or more and blamed the drive.
#[test]
fn a_copy_name_shortens_its_stem_to_fit_and_keeps_the_word_and_extension() {
    let cases = [
        "L".repeat(251) + ".txt",
        "é".repeat(125) + ".txt",
        "é".repeat(127),
        "漢".repeat(85),
        "s.".to_string() + &"x".repeat(253),
    ];
    for name in &cases {
        for n in [1, 2, 999] {
            let made = copy_name(&Path::new("/x").join(name), "copy", n)
                .expect("a long name still has a copy name");
            let made = made
                .to_str()
                .expect("a UTF-8 name is cut on a character boundary");
            assert!(made.len() <= NAME_MAX, "{} bytes for {made}", made.len());
            let word = if n == 1 {
                " copy".to_string()
            } else {
                format!(" copy {n}")
            };
            assert!(made.contains(&word), "the word survives: {made}");
        }
    }
    assert!(
        copy_name(Path::new(&("/x/".to_string() + &cases[0])), "copy", 1)
            .unwrap()
            .as_bytes()
            .ends_with(b" copy.txt")
    );
    assert_eq!(
        copy_name(Path::new("/x/photo.jpg"), "copy", 1).unwrap(),
        "photo copy.jpg",
        "a short name is untouched"
    );
}

// A long name that is not UTF-8 is cut by its own bytes, never its lossy spelling, and never inside a character.
#[test]
fn a_long_name_that_is_not_utf8_shortens_by_its_own_bytes() {
    // 99 raw bytes put the 244-byte room on the second byte of an é, so the cut must back off one byte.
    let mut name = b"\xff".repeat(99);
    name.extend("é".repeat(76).as_bytes());
    name.extend(b".txt");
    let original = Path::new("/x").join(OsStr::from_bytes(&name));
    let made = copy_name(&original, "copy", 2).expect("a long raw name still has a copy name");
    let made = made.as_bytes();
    assert_eq!(made.len(), 243 + b" copy 2.txt".len(), "{made:?}");
    assert!(made.starts_with(&b"\xff".repeat(99)) && made.ends_with(b"\xc3\xa9 copy 2.txt"));
    let run = b"\x80".repeat(255);
    let made = copy_name(&Path::new("/x").join(OsStr::from_bytes(&run)), "copy", 1)
        .expect("a run of stray continuation bytes is no character, so it is cut where it falls");
    assert_eq!(made.as_bytes(), [&run[..250], b" copy"].concat());
}

// S4-8 end to end: a 255-byte name duplicates beside itself.
#[test]
fn a_two_hundred_fifty_five_byte_name_duplicates() {
    let d = TestDir::new("longdup");
    let original = d.file(&("é".repeat(125) + ".txtx"), "body");
    let (outcome, steps) = duplicate(&original);
    let made = outcome.map_err(|e| e.msg).expect("the copy name fits");
    assert_eq!(std::fs::read_to_string(&made).unwrap(), "body");
    assert!(made.file_name().unwrap().len() <= NAME_MAX && steps.len() == 1);
}
