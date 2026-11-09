// Copy names for long originals (soak S4-8).
use crate::backend::ops::{copy_name, duplicate};
use crate::backend::testdir::TestDir;
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
            .ends_with(" copy.txt")
    );
    assert_eq!(
        copy_name(Path::new("/x/photo.jpg"), "copy", 1).unwrap(),
        "photo copy.jpg",
        "a short name is untouched"
    );
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
