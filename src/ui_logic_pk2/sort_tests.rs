//! Port of requestSort cases: remembering, default leaf, guards.
use super::sort::*;

#[test]
fn a_folder_sort_remembers_and_moves_the_default_leaf() {
    assert_eq!(
        request_sort(("name", false), true, false, "/home/gm/Downloads", true, ("mtime", true)),
        Some(SortEffect {
            sort_by: "mtime".into(),
            sort_desc: true,
            remember: Some(("/home/gm/Downloads".into(), "mtime".into(), true)),
            leaf_key: "date".into(),
            leaf_reverse: true,
        })
    );
}

#[test]
fn with_remembering_off_no_folder_is_written_while_the_default_still_follows() {
    assert_eq!(
        request_sort(("name", false), true, false, "/home/gm/Downloads", false, ("size", false)),
        Some(SortEffect {
            sort_by: "size".into(),
            sort_desc: false,
            remember: None,
            leaf_key: "size".into(),
            leaf_reverse: false,
        })
    );
}

#[test]
fn recent_and_pathless_sorts_move_the_window_but_remember_no_folder() {
    for (recent, path) in [(true, "/home/gm/Downloads"), (false, "")] {
        let effect = request_sort(("name", false), true, recent, path, true, ("size", true)).expect("the window sort still changes");
        assert_eq!((effect.sort_by.as_str(), effect.sort_desc), ("size", true));
        assert_eq!(effect.remember, None);
    }
}

#[test]
fn the_order_it_has_and_an_unsortable_window_ask_for_nothing() {
    assert_eq!(request_sort(("mtime", true), true, false, "/home/gm/Downloads", true, ("mtime", true)), None);
    assert_eq!(request_sort(("name", false), false, false, "/home/gm/Downloads", true, ("size", false)), None);
}

#[test]
fn a_key_the_schema_shares_passes_through_as_is() {
    let effect = request_sort(("name", false), true, false, "/d", true, ("size", false)).expect("a new order sorts");
    assert_eq!((effect.leaf_key.as_str(), effect.leaf_reverse), ("size", false));
    assert_eq!(effect.remember, Some(("/d".into(), "size".into(), false)));
}
