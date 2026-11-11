//! Picker.js sort cases (156-172): three orders, kind never reached.
use flea::ui_logic_picker_listing::path::view_switch;
use flea::ui_logic_picker_listing::sort::{
    picker_column_order, picker_next_order, picker_reverse_order, request_sort, PickerOrder,
    PICKER_ORDERS,
};

fn order(key: &str, desc: bool) -> PickerOrder {
    PickerOrder {
        key: key.to_owned(),
        desc,
    }
}

#[test]
fn the_chooser_offers_the_three_columns_it_draws() {
    assert_eq!(PICKER_ORDERS, ["name", "size", "mtime"]);
}

#[test]
fn a_click_on_another_column_starts_it_ascending() {
    assert_eq!(
        picker_column_order(&order("name", false), "size"),
        Some(order("size", false))
    );
}

#[test]
fn a_click_on_the_sorted_column_reverses_it() {
    assert_eq!(
        picker_column_order(&order("size", false), "size"),
        Some(order("size", true))
    );
}

#[test]
fn and_a_second_click_puts_it_back() {
    assert_eq!(
        picker_column_order(&order("size", true), "size"),
        Some(order("size", false))
    );
}

#[test]
fn rev_r3_a_column_the_chooser_does_not_offer_asks_for_nothing() {
    assert_eq!(picker_column_order(&order("name", false), "kind"), None);
    assert_eq!(picker_column_order(&order("name", false), "frob"), None);
}

#[test]
fn s_steps_name_size_modified_then_wraps() {
    assert_eq!(
        picker_next_order(&order("name", false)),
        order("size", false)
    );
    assert_eq!(
        picker_next_order(&order("size", false)),
        order("mtime", false)
    );
}

#[test]
fn rev_r3_s_wraps_from_modified_to_name_never_onto_kind() {
    assert_eq!(
        picker_next_order(&order("mtime", false)),
        order("name", false)
    );
    assert_eq!(
        picker_next_order(&order("mtime", true)),
        order("name", false)
    );
}

#[test]
fn capital_s_reverses_the_order_the_listing_is_in() {
    assert_eq!(
        picker_reverse_order(&order("mtime", false)),
        order("mtime", true)
    );
    assert_eq!(
        picker_reverse_order(&order("kind", true)),
        order("kind", false)
    );
}

// picker.js:169: kind desc steps to name asc.
#[test]
fn s_from_an_inherited_kind_order_starts_over_at_name() {
    assert_eq!(
        picker_next_order(&order("kind", true)),
        order("name", false)
    );
}

#[test]
fn a_view_round_trip_keeps_the_sort_the_header_click_set() {
    // checkKept: header sort survives each view switch.
    let effect = request_sort(
        ("name", false),
        true,
        false,
        "/home/gm/Downloads",
        true,
        ("size", false),
    )
    .expect("a new order sorts");
    let mut view = "list";
    let mut seen = vec![view];
    for next in ["grid", "list", "grid", "list"] {
        view = view_switch(view, next).expect("the switch moves");
        seen.push(view);
        assert_eq!((effect.sort_by.as_str(), effect.sort_desc), ("size", false));
    }
    assert_eq!(seen, ["list", "grid", "list", "grid", "list"]);
}
