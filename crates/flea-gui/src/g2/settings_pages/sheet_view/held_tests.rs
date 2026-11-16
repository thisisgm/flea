//! Where the held list's second column starts: between groups where it can, never leaving a heading at a column's foot.
use super::{split, Item, HEAD_SLOT, KEY_ROW, SPLIT_AFTER};

/// A list of groups of these sizes, each under its heading.
fn list(groups: &[usize]) -> (Vec<Item>, usize) {
    let mut items = Vec::new();
    let mut rows = 0;
    for (group, size) in groups.iter().enumerate() {
        items.push(Item::Head(Some(group)));
        for _ in 0..*size {
            items.push(Item::Row(rows));
            rows += 1;
        }
    }
    (items, rows)
}

#[test]
fn a_short_list_stays_in_one_column() {
    let (items, rows) = list(&[3, SPLIT_AFTER - 3]);
    assert_eq!(split(&items, rows, 1000.), items.len());
}

#[test]
fn a_longer_list_splits_between_groups_where_the_columns_come_out_most_even() {
    let (items, rows) = list(&[4, 6, 5, 6]);
    let at = split(&items, rows, 1000.);
    assert!(
        matches!(items[at], Item::Head(Some(2))),
        "after the first two groups"
    );
}

#[test]
fn a_list_too_tall_for_its_room_still_splits_between_groups_and_scrolls() {
    let (items, rows) = list(&[5, 15]);
    let at = split(&items, rows, HEAD_SLOT + 6. * KEY_ROW);
    assert!(matches!(items[at], Item::Head(Some(1))), "{at}");
}

#[test]
fn a_single_group_splits_in_half_without_an_orphaned_heading() {
    let (items, rows) = list(&[20]);
    let at = split(&items, rows, 1000.);
    assert!(matches!(items[at], Item::Row(10)), "{at}");
    let (items, rows) = list(&[SPLIT_AFTER + 1]);
    let at = split(&items, rows, 1000.);
    assert!(!matches!(items[at - 1], Item::Head(_)), "{at}");
}
