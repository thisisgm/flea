// 0401-settings-hover and 0401-list-nohover: the pointer's row lifts only under a stored true; the pointer fact stays.
use super::HoverWash;
use crate::list::{RowState, RowStates};
use flea::jsondoc::parse;

#[test]
fn only_a_stored_true_ticks_highlight_on_hover() {
    for (text, on) in [
        ("{}", false),
        (r#"{"highlightHover":false}"#, false),
        (r#"{"highlightHover":"yes"}"#, false),
        (r#"{"highlightHover":true}"#, true),
    ] {
        assert_eq!(
            HoverWash::read(&parse(text).unwrap()),
            HoverWash(on),
            "{text}"
        );
    }
    assert_eq!(
        HoverWash::default(),
        HoverWash(false),
        "an unset global washes nothing"
    );
}

#[test]
fn an_unticked_pointer_row_keeps_its_rest_look_and_its_pointer_fact() {
    let states = |hover_wash| RowStates {
        listing: 1,
        hovered: Some(2),
        hover_wash,
        ..RowStates::default()
    };
    let rest = states(false).row(2, 1);
    assert!(rest.hovered, "probes and tips still read the pointer");
    assert!(
        !rest.hover_lit && !rest.lifted(),
        "no wash and no lifted ink"
    );
    let ticked = states(true).row(2, 1);
    assert!(ticked.hover_lit && ticked.lifted());
    assert_eq!(
        states(true).row(3, 1),
        RowState::default(),
        "only the pointer's row"
    );
}
