use super::*;
use crate::backend::testdir::TestDir;

const BOARD: &str = "The quick brown fox jumps over the lazy dog";

// One sfnt with only a cmap: a (3,1) format 4 record over the given segments.
pub(crate) fn font_with(segments: &[(u16, u16)]) -> Vec<u8> {
    let mut segs = segments.to_vec();
    segs.push((0xFFFF, 0xFFFF));
    let n = segs.len() as u16;
    let mut sub = Vec::new();
    for v in [4u16, 16 + 8 * n, 0, 2 * n, 0, 0, 0] {
        sub.extend(v.to_be_bytes());
    }
    segs.iter()
        .for_each(|(_, end)| sub.extend(end.to_be_bytes()));
    sub.extend(0u16.to_be_bytes());
    segs.iter()
        .for_each(|(start, _)| sub.extend(start.to_be_bytes()));
    segs.iter().for_each(|_| sub.extend(1u16.to_be_bytes()));
    segs.iter().for_each(|_| sub.extend(0u16.to_be_bytes()));
    let mut cmap = Vec::new();
    for v in [0u16, 1, 3, 1] {
        cmap.extend(v.to_be_bytes());
    }
    cmap.extend(12u32.to_be_bytes());
    cmap.extend(sub);
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    for v in [1u16, 16, 0, 0] {
        out.extend(v.to_be_bytes());
    }
    out.extend(b"cmap");
    out.extend(0u32.to_be_bytes());
    out.extend(28u32.to_be_bytes());
    out.extend((cmap.len() as u32).to_be_bytes());
    out.extend(cmap);
    out
}

#[test]
fn a_font_that_covers_the_board_sentence_keeps_it() {
    assert_eq!(choose(BOARD, &[(0x20, 0x7e)]), BOARD);
    // An unreadable cmap keeps the sentence too.
    assert_eq!(choose(BOARD, &[]), BOARD);
}

#[test]
fn a_georgian_font_draws_the_georgian_alphabet() {
    let d = TestDir::new("font-sample-georgian");
    let path = d.join("Georgian.ttf");
    std::fs::write(&path, font_with(&[(0x20, 0x40), (0x10D0, 0x10FF)])).unwrap();
    let ranges = ranges(&path);
    assert_eq!(ranges, vec![(0x20, 0x40), (0x10D0, 0x10FF)]);
    let sample = choose(BOARD, &ranges);
    assert!(
        sample.starts_with('\u{10D0}') && sample.chars().count() == 33,
        "{sample}"
    );
}

#[test]
fn a_cyrillic_font_draws_its_pangram_and_any_other_script_its_first_letters() {
    let cyrillic = choose(BOARD, &[(0x20, 0x40), (0x0400, 0x04FF)]);
    assert!(cyrillic.starts_with('\u{0421}'), "{cyrillic}");
    let ethiopic = choose(BOARD, &[(0x1200, 0x137C)]);
    assert_eq!(ethiopic.chars().count(), RUN);
    assert!(ethiopic
        .chars()
        .all(|c| ('\u{1200}'..='\u{137C}').contains(&c)));
    // A font with no letters at all keeps the sentence rather than drawing nothing.
    assert_eq!(choose(BOARD, &[(0x2500, 0x257F)]), BOARD);
}

#[test]
fn a_file_with_no_cmap_reads_no_ranges() {
    let d = TestDir::new("font-sample-none");
    assert!(ranges(&d.file("Fake.ttf", "not a font")).is_empty());
}

// The installed font the soak found blank, when this box has it.
#[test]
fn noto_serif_georgian_gets_a_georgian_sample() {
    let path = Path::new("/usr/share/fonts/noto/NotoSerifGeorgian-Regular.ttf");
    if !path.is_file() {
        return;
    }
    let sample = choose(BOARD, &ranges(path));
    assert!(
        sample.chars().all(|c| c.is_whitespace()
            || ('\u{10A0}'..='\u{10FF}').contains(&c)
            || ('\u{1C90}'..='\u{1CBF}').contains(&c)),
        "{sample}"
    );
}

// A crafted cmap: groups past Unicode, reversed, and the same private-use block many times over.
#[test]
fn crafted_groups_are_clamped_and_joined() {
    let mut groups = vec![(0x11_0000, u32::MAX), (0x50, 0x40), (0x10_FFF0, u32::MAX)];
    groups.extend(std::iter::repeat_n((0xE000, 0xF8FF), 1000));
    groups.push((0xF900, 0xF9FF));
    assert_eq!(
        merged(groups),
        vec![(0xE000, 0xF9FF), (0x10_FFF0, 0x10_FFFF)]
    );
}
