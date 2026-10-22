use super::*;
use crate::jsondoc::{self, Json};
use crate::ui_logic_st1::test_support::say;

// The board's "02 Historical layout projection" table, verbatim: base, bodySmall, caption, paddingY, rowHeight, iconSize, mark.
const BOARD: [(u32, u32, u32, u32, u32, u32, f64); 7] = [
    (9, 8, 7, 5, 24, 14, 11.60),
    (10, 9, 8, 5, 26, 16, 13.05),
    (11, 10, 9, 6, 30, 18, 14.50),
    (12, 11, 10, 6, 32, 20, 15.95),
    (14, 13, 12, 7, 37, 23, 18.85),
    (16, 15, 13, 8, 43, 27, 21.75),
    (20, 18, 17, 10, 52, 32, 26.10),
];

fn stored(mode: &str) -> Json {
    jsondoc::parse(mode).expect("fixture parses")
}

fn number(text: &str) -> Json {
    Json::Num(text.into())
}

fn mode(value: Json) -> Json {
    Json::Obj(vec![("mode".into(), value)])
}

#[test]
fn stops_are_the_board_list_across_the_oem_range() {
    assert_eq!(STOPS.len(), 7);
    assert_eq!(STOPS.to_vec(), vec![9, 10, 11, 12, 14, 16, 20]);
    assert_eq!((STOPS[0], STOPS[STOPS.len() - 1]), (9, 20));
    assert!(!STOPS.contains(&13));
    assert_eq!([15, 17, 18, 19].iter().filter(|px| STOPS.contains(px)).count(), 0);
}

#[test]
fn nearest_snaps_to_a_stop_and_ties_take_the_smaller() {
    assert_eq!(nearest(19.0), 20);
    assert_eq!(nearest(13.0), 12);
    assert_eq!(nearest(15.0), 14);
    assert_eq!(nearest(4.0), 9);
    assert_eq!(nearest(64.0), 20);
    assert_eq!(nearest(14.0), 14);
    // Outside the oracle's checks: NaN and Infinity both fall to the first stop, as the strict comparison does.
    assert_eq!(nearest(f64::NAN), 9);
    assert_eq!(nearest(f64::INFINITY), 9);
}

#[test]
fn modes_follow_and_draw_at_the_right_size() {
    assert!(following(Stored::Follow));
    assert_eq!(to_json(&Stored::Follow), mode(Json::Str("system".into())));
    assert!(!following(Stored::Stop(16)));
    assert!(following(parse(&Json::Null)));
    assert_eq!(effective(Stored::Follow, 14), 14);
    assert_eq!(effective(Stored::Follow, 16), 16);
    assert_eq!(effective(Stored::Stop(20), 14), 20);
}

#[test]
fn pin_keeps_the_size_on_screen_and_step_moves_one_stop() {
    assert_eq!(pin(Stored::Follow, 14), Stored::Stop(14));
    assert_eq!(pin(Stored::Follow, 13), Stored::Stop(12));
    assert_eq!(pin(Stored::Stop(10), 14), Stored::Stop(10));
    assert_eq!(to_json(&stepped(Stored::Follow, 14, 1)), mode(number("16")));
    assert_eq!(stepped(Stored::Follow, 14, -1), Stored::Stop(12));
    assert_eq!(stepped(Stored::Stop(12), 14, 1), Stored::Stop(14));
    assert_eq!(stepped(Stored::Stop(20), 14, 1), Stored::Stop(20));
    assert_eq!(stepped(Stored::Stop(9), 14, -1), Stored::Stop(9));
}

#[test]
fn two_steps_up_then_back_return_to_the_start() {
    let mut walked = Stored::Follow;
    for _ in 0..2 {
        walked = stepped(walked, 14, 1);
    }
    assert_eq!(walked, Stored::Stop(20));
    for _ in 0..2 {
        walked = stepped(walked, 14, -1);
    }
    assert_eq!(walked, Stored::Stop(14));
}

#[test]
fn stored_shapes_parse_to_a_stop_or_follow() {
    assert_eq!(parse(&stored(r#"{"mode":16}"#)), Stored::Stop(16));
    assert_eq!(parse(&stored(r#"{"mode":13}"#)), Stored::Stop(12));
    assert_eq!(parse(&stored(r#"{"mode":0}"#)), Stored::Follow);
    assert_eq!(parse(&stored(r#"{"mode":"big"}"#)), Stored::Follow);
    assert_eq!(parse(&stored(r#"{"mode":"override","px":16}"#)), Stored::Follow);
    assert_eq!(parse(&Json::Null), Stored::Follow);
    assert_eq!(to_json(&Stored::Stop(16)), mode(number("16")));
    assert_eq!(to_json(&parse(&stored(r#"{"mode":"system"}"#))), mode(Json::Str("system".into())));
}

#[test]
fn stored_numeric_strings_and_other_json_kinds_follow_number() {
    // Number("16") is 16, so a numeric string counts; Number(true) is 1 and snaps to the floor.
    assert_eq!(parse(&stored(r#"{"mode":"16"}"#)), Stored::Stop(16));
    assert_eq!(parse(&stored(r#"{"mode":" 16 "}"#)), Stored::Stop(16));
    assert_eq!(parse(&stored(r#"{"mode":""}"#)), Stored::Follow);
    assert_eq!(parse(&stored(r#"{"mode":true}"#)), Stored::Stop(9));
    assert_eq!(parse(&stored(r#"{"mode":[16]}"#)), Stored::Stop(16));
    assert_eq!(parse(&stored(r#"{"mode":[]}"#)), Stored::Follow);
    assert_eq!(parse(&stored(r#"{"mode":{}}"#)), Stored::Follow);
    assert_eq!(parse(&stored(r#"{"mode":"0x10"}"#)), Stored::Stop(16));
    assert_eq!(parse(&stored(r#"{"mode":"Infinity"}"#)), Stored::Stop(9));
    assert_eq!(parse(&stored(r#"{"mode":"inf"}"#)), Stored::Follow);
    assert_eq!(parse(&stored(r#"{"mode":"1e1"}"#)), Stored::Stop(10));
}

#[test]
fn board_table_rows_match_the_formulas() {
    for (base, small, caption_px, padding, row_height, icon, mark) in BOARD {
        assert_eq!(body_small(base), small, "bodySmall at {base}");
        assert_eq!(caption(base), caption_px, "caption at {base}");
        // Theme.qml's paddingY is the board's round(6 x base / 12), with 12 the stock anchor.
        let padding_y = (f64::from(6 * base) / 12.0).round() as u32;
        assert_eq!(padding_y, padding, "paddingY at {base}");
        // Theme.qml: rowHeight = round(bodySmall x 1.8) + 2 x paddingY; iconSize = rowHeight - 2 x paddingY.
        let computed_row = (f64::from(small) * 1.8).round() as u32 + 2 * padding_y;
        assert_eq!(computed_row, row_height, "rowHeight at {base}");
        assert_eq!(computed_row - 2 * padding_y, icon, "iconSize at {base}");
        // Theme.qml: markSize = bodySmall x 1.45, compared at the board's two decimals.
        let computed_mark = (f64::from(small) * 1.45 * 100.0).round() / 100.0;
        assert_eq!(computed_mark, mark, "mark at {base}");
    }
    let tabulated: Vec<u32> = BOARD.iter().map(|row| row.0).collect();
    assert!(tabulated.iter().all(|base| STOPS.contains(base)));
    assert_eq!(tabulated.len(), STOPS.len());
}

#[test]
fn rescaled_padding_reaches_every_board_row() {
    // Theme.qml rescales Omarchy's controlPaddingY, space(6) = 7 at this box's base of 14.
    for (base, _, _, padding, _, _, _) in BOARD {
        assert_eq!((f64::from(7 * base) / 14.0).round() as u32, padding, "at {base}");
    }
}

#[test]
fn announce_names_the_size_and_the_way_back() {
    let override_text = announce_for(Stored::Stop(16), 14, "default");
    assert_eq!(override_text.key(), Some("settings.textsize.override"));
    assert_eq!(say(&override_text), "Text size 16px. Ctrl+Shift+0 follows Omarchy again");
    assert_eq!(say(&announce_for(Stored::Follow, 14, "default")), "Text size follows Omarchy, 14px");
    assert_eq!(announce_for(Stored::Follow, 14, "default").key(), Some("settings.textsize.follows_omarchy"));
}

#[test]
fn announce_drops_the_way_back_when_no_key_is_bound() {
    let alone = announce(Stored::Stop(16), 14, "");
    assert_eq!(alone.key(), Some("settings.textsize.override_alone"));
    assert_eq!(say(&alone), "Text size 16px");
}

#[test]
fn direction_maps_each_action_to_one_delta() {
    assert_eq!(direction("textSizeUp"), 1);
    assert_eq!(direction("textSizeDown"), -1);
    assert_eq!(direction("textSizeReset"), 0);
}

#[test]
fn body_and_ratios_clamp_at_one() {
    assert_eq!(body(14), 14);
    assert_eq!(body_small(1), 1);
    assert_eq!(caption(1), 1);
    assert_eq!(caption(0), 1);
}
