// Ported from tests/js/mddraw.js and tests/js/mdnest.js: where nested entries draw, and what a chunk of a long list carries over.
use super::super::{blocks, Block, Ctx, Words};
use super::*;

// A fake font: every character is 8 px wide, a bullet included.
const CHAR_PX: f32 = 8.0;
const GAP: f32 = 6.0;
const TEXT0: f32 = CHAR_PX + GAP;

fn advance(text: &str) -> f32 {
    text.chars().count() as f32 * CHAR_PX
}

fn lists(doc: &str) -> Vec<List> {
    let words = Words::english();
    let ctx = Ctx {
        dir: "/home/gm/notes",
        chrome: "#181825",
        ink: "#c0caf5",
        words: &words,
    };
    blocks(doc, &ctx)
        .into_iter()
        .filter_map(|block| match block {
            Block::List(list) => Some(list),
            _ => None,
        })
        .collect()
}

fn cells(doc: &str) -> Vec<Cell> {
    layout(&lists(doc)[0], &advance, GAP)
}

fn markers(cells: &[Cell]) -> Vec<&str> {
    cells.iter().map(|cell| cell.marker.as_str()).collect()
}

fn gaps(cells: &[Cell]) -> Vec<bool> {
    cells.iter().map(|cell| cell.gap).collect()
}

// Numbered entries from..=to, one of them holding a nested bullet.
fn numbered(from: u32, to: u32, nested_at: Option<u32>) -> String {
    let items: Vec<String> = (from..=to)
        .map(|n| {
            let child = if Some(n) == nested_at {
                "\n    - child"
            } else {
                ""
            };
            format!("{n}. item{child}")
        })
        .collect();
    items.join("\n") + "\n"
}

#[test]
fn a_depth_draws_its_marker_at_its_parent_s_text_column() {
    let nested = cells("- one\n  - two\n    - three\n- four\n  - five\n");
    let (disc, circle, square) = (BULLETS[0], BULLETS[1], BULLETS[2]);
    assert_eq!(markers(&nested), vec![disc, circle, square, disc, circle]);
    let columns: Vec<f32> = nested.iter().map(|cell| cell.x).collect();
    assert_eq!(columns, vec![0.0, TEXT0, 2.0 * TEXT0, 0.0, TEXT0]);
    assert_eq!(
        cells("- a\n  - b\n    - c\n      - d\n")[3].marker,
        square,
        "a fourth level keeps the square"
    );
    assert!(
        !nested.iter().any(|cell| cell.gap),
        "a tight list has no gaps"
    );
}

#[test]
fn an_ordered_parent_s_marker_width_sets_its_child_s_column() {
    let ordered = cells("9. a\n   - b\n10. c\n");
    assert_eq!(markers(&ordered), vec!["9.", BULLETS[1], "10."]);
    assert_eq!(
        ordered[0].w,
        3.0 * CHAR_PX,
        "the entries share the widest marker column"
    );
    assert_eq!(ordered[1].x, 3.0 * CHAR_PX + GAP);
    let flat = List {
        ordered: true,
        start: 8,
        items: vec!["a".into(), "b".into(), "c".into()],
        ..List::default()
    };
    assert_eq!(
        markers(&layout(&flat, &advance, GAP)),
        vec!["8.", "9.", "10."],
        "a flat list reads its numbering without the parser's markers"
    );
}

#[test]
fn a_loose_list_gaps_every_entry_but_the_first_and_a_continuation_has_no_marker() {
    assert_eq!(gaps(&cells("- a\n\n- b\n\n- c\n")), vec![false, true, true]);
    assert_eq!(
        gaps(&cells("- a\n\n  - b\n- c\n")),
        vec![false, true, true],
        "a loose item gaps its nested list"
    );
    let tail = cells("- a\n  - b\n\n  tail\n");
    let placed: Vec<(&str, f32)> = tail[1..]
        .iter()
        .map(|cell| (cell.marker.as_str(), cell.x))
        .collect();
    assert_eq!(placed, vec![(BULLETS[1], TEXT0), ("", 0.0)]);
}

#[test]
fn a_later_chunk_keeps_its_depths_markers_and_the_widest_number_s_column() {
    let chunks = lists(&numbered(1, 40, Some(33)));
    let lengths: Vec<usize> = chunks.iter().map(|chunk| chunk.items.len()).collect();
    assert_eq!(lengths, vec![32, 9]);
    let depths = chunks[1].depths.clone().unwrap_or_default();
    assert_eq!(depths, vec![0, 1, 0, 0, 0, 0, 0, 0, 0]);
    let carried = chunks[1].markers.clone().unwrap_or_default();
    assert_eq!(carried[..2], ["33.".to_string(), BULLETS[0].to_string()]);
    assert_eq!(layout(&chunks[0], &advance, GAP)[0].w, 3.0 * CHAR_PX);
    // 68 to 99 take two digits in the first chunk and 100 to 107 three in the second, so only a carried width lines both up.
    let wide = lists(&numbered(68, 107, None));
    assert_eq!(layout(&wide[0], &advance, GAP)[0].w, 4.0 * CHAR_PX);
    assert_eq!(layout(&wide[1], &advance, GAP)[0].w, 4.0 * CHAR_PX);
    let orphan = List {
        items: vec!["x".into()],
        depths: Some(vec![2]),
        markers: Some(vec![BULLETS[0].into()]),
        gaps: Some(vec![false]),
        ..List::default()
    };
    assert_eq!(
        layout(&orphan, &advance, GAP)[0].x,
        2.0 * TEXT0,
        "a chunk that opens inside a nested list counts a disc column per level"
    );
}

#[test]
fn a_seam_keeps_the_parent_s_column_and_a_loose_list_s_gap() {
    let seam = lists(&numbered(1, 40, Some(32)));
    let first = layout(&seam[0], &advance, GAP);
    let parent = first.last().expect("the first chunk draws entries");
    let carried = layout(&seam[1], &advance, GAP);
    assert_eq!(seam[1].depths.as_ref().map(|depths| depths[0]), Some(1));
    assert_eq!(carried[0].x, parent.x + parent.w + GAP);
    let loose: Vec<String> = (1..=40).map(|n| format!("- item {n}\n")).collect();
    let chunks = lists(&(loose.join("\n") + "\n"));
    assert_eq!(chunks.len(), 2, "a loose list splits in chunks");
    assert!(!layout(&chunks[0], &advance, GAP)[0].gap);
    assert!(
        layout(&chunks[1], &advance, GAP)[0].gap,
        "the first entry of a later chunk keeps its gap above it"
    );
}
