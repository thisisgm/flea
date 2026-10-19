use super::*;
use flea_gui::tabs::{
    bar::{insertion_at, rail_reorder, step},
    state::{pack, Tab},
};

fn three(path: &str, index: usize) -> Side {
    let mut side = Side::new(Fake::at(path));
    side.state.tabs = Some(pack(
        vec![Tab::at("/tmp/a"), Tab::at("/tmp/b"), Tab::at("/tmp/c")],
        index,
        None,
    ));
    side
}

fn order_of(side: &Side) -> String {
    side.paths().join(",")
}

#[test]
fn braces_move_the_current_tab_one_place_and_list_nothing() {
    let mut left = three("/tmp/b", 1);
    left.act("tabMoveLeft");
    assert_eq!(
        order_of(&left),
        "/tmp/b,/tmp/a,/tmp/c",
        "{{ swaps the current tab with its left neighbour"
    );
    assert_eq!(left.state.current(), 0, "and the current tab stays current");
    assert!(left.pane.listed.is_empty(), "and moving lists nothing");

    let mut right = three("/tmp/b", 1);
    right.act("tabMoveRight");
    assert_eq!(
        order_of(&right),
        "/tmp/a,/tmp/c,/tmp/b",
        "}} swaps the current tab with its right neighbour"
    );
    assert_eq!(
        right.state.current(),
        2,
        "and the current tab stays current"
    );
    assert!(
        right.pane.listed.is_empty(),
        "and moving lists nothing either"
    );

    let mut first = three("/tmp/a", 0);
    first.act("tabMoveLeft");
    assert_eq!(
        (order_of(&first), first.state.current()),
        ("/tmp/a,/tmp/b,/tmp/c".into(), 0),
        "{{ on the first tab is a no-op"
    );

    let mut last = three("/tmp/c", 2);
    last.act("tabMoveRight");
    assert_eq!(
        (order_of(&last), last.state.current()),
        ("/tmp/a,/tmp/b,/tmp/c".into(), 2),
        "}} on the last tab is a no-op"
    );
}

#[test]
fn a_drag_lands_at_either_end_and_the_current_tab_stays_current() {
    let mut to_first = three("/tmp/c", 2);
    to_first.state.move_tab(&mut to_first.pane, 2, 0);
    assert_eq!(
        order_of(&to_first),
        "/tmp/c,/tmp/a,/tmp/b",
        "a drag to the near end lands first"
    );
    assert_eq!(
        to_first.state.current(),
        0,
        "and the dragged tab stays current"
    );

    let mut to_last = three("/tmp/a", 0);
    to_last.state.move_tab(&mut to_last.pane, 0, 2);
    assert_eq!(
        order_of(&to_last),
        "/tmp/b,/tmp/c,/tmp/a",
        "a drag to the far end lands last"
    );
    assert_eq!(
        to_last.state.current(),
        2,
        "and the dragged tab stays current there too"
    );

    let mut behind = three("/tmp/c", 2);
    behind.state.move_tab(&mut behind.pane, 0, 2);
    assert_eq!(
        (order_of(&behind), behind.state.current()),
        ("/tmp/b,/tmp/c,/tmp/a".into(), 1),
        "moving a tab across the current one keeps the same tab current"
    );

    let mut sorted = three("/tmp/b", 1);
    sorted.state.tabs.as_mut().unwrap().items[0].order = order("size", false);
    sorted.state.tabs.as_mut().unwrap().items[2].order = order("mtime", false);
    sorted.act("tabMoveRight");
    let keys: Vec<&str> = sorted
        .state
        .tabs
        .as_ref()
        .unwrap()
        .items
        .iter()
        .map(|tab| tab.order.key)
        .collect();
    assert_eq!(
        keys,
        ["size", "mtime", "name"],
        "each tab keeps its own sort order across a move"
    );

    let mut single = Side::new(Fake::at("/tmp/only"));
    single.act("tabMoveLeft");
    single.act("tabMoveRight");
    assert!(
        single.state.tabs.is_none(),
        "a single tab moves nowhere and gains no tab state"
    );

    let mut bad = three("/tmp/b", 1);
    bad.state.move_tab(&mut bad.pane, usize::MAX, 0);
    bad.state.move_tab(&mut bad.pane, 0, 9);
    bad.state.move_tab(&mut bad.pane, 1, 1);
    assert_eq!(
        (order_of(&bad), bad.state.current()),
        ("/tmp/a,/tmp/b,/tmp/c".into(), 1),
        "an out-of-range or empty move changes nothing"
    );
}

#[test]
fn the_insertion_point_and_the_step_clamp_at_both_ends() {
    assert_eq!(
        insertion_at(-20., 100., 3),
        0,
        "an insertion left of the strip clamps to the near end"
    );
    assert_eq!(
        insertion_at(49., 100., 3),
        0,
        "an insertion inside the first tab lands before it"
    );
    assert_eq!(
        insertion_at(50., 100., 3),
        1,
        "an insertion past a tab middle lands after it"
    );
    assert_eq!(
        insertion_at(999., 100., 3),
        3,
        "an insertion past the last tab clamps to the far end"
    );
    assert_eq!(step(0, -1, 3), 0, "a step left clamps at the near end");
    assert_eq!(step(2, 1, 3), 2, "a step right clamps at the far end");
}

#[test]
fn a_move_while_a_listing_is_out_keeps_what_the_current_tabs_landing_owes() {
    let mut busy = three("/tmp/b", 1);
    busy.pane.busy = true;
    {
        let pending = &mut busy.state.tabs.as_mut().unwrap().pending;
        pending.order = Some(order("size", false));
        pending.cursor = Some(7);
    }
    busy.state.move_tab(&mut busy.pane, 2, 0);
    assert_eq!(
        (order_of(&busy), busy.pane.said.join("")),
        ("/tmp/c,/tmp/a,/tmp/b".into(), String::new()),
        "a move while a listing is out reorders and says nothing"
    );
    let pending = &busy.state.tabs.as_ref().unwrap().pending;
    assert_eq!(
        (pending.order.map(|order| order.key), pending.cursor),
        (Some("size"), Some(7)),
        "and it keeps the pending restore"
    );

    let mut closing = three("/tmp/b", 1);
    closing.pane.busy = true;
    closing.state.tabs.as_mut().unwrap().pending.cursor = Some(6);
    closing.state.close_at(&mut closing.pane, 0);
    assert_eq!(
        (
            closing.state.tabs.as_ref().unwrap().pending.cursor,
            closing.state.current()
        ),
        (Some(6), 0),
        "closing another tab leaves what the current tab's landing owes"
    );
}

#[test]
fn the_rail_favourite_drag_answers_its_landing_and_its_line_from_one_helper() {
    let down = rail_reorder(30., 1, 30., 5);
    assert_eq!(down.to, 2, "a rail drag down one lands one lower");
    assert_eq!(down.line, 3, "and draws the line below it");
    let up = rail_reorder(-30., 1, 30., 5);
    assert_eq!(up.to, 0, "a rail drag up one lands one higher");
    assert_eq!(up.line, 0, "and draws the line above it");
    assert_eq!(
        rail_reorder(-999., 0, 30., 5).to,
        0,
        "a rail drag past the near end clamps its landing"
    );
    assert_eq!(
        rail_reorder(-999., 0, 30., 5).line,
        0,
        "and its line clamps too"
    );
    assert_eq!(
        rail_reorder(999., 4, 30., 5).to,
        4,
        "a rail drag past the far end clamps its landing"
    );
    assert_eq!(
        rail_reorder(999., 4, 30., 5).line,
        5,
        "and its line rests past the last row"
    );
    assert_eq!(
        rail_reorder(10., 1, 30., 5).to,
        1,
        "a sub-half-row rail drag lands nowhere"
    );
}
