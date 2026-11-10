//! The default quiet hover and the optional wash preserve row ownership across listings.
use super::harness::{held, open, real_cx, uniform};
use flea::jsondoc::parse;
use flea_gui::list::{hover_wash::HoverWash, RowStates};
use std::rc::Rc;

#[test]
fn row_states_resolve_per_row_and_never_outlive_their_listing() {
    let shipped = HoverWash::read(&parse(flea::uischema::DEFAULTS).unwrap()).0;
    assert!(
        !shipped,
        "0401-settings-hover: a fresh install washes nothing under the pointer"
    );
    for hover_wash in [shipped, true] {
        let mut cx = real_cx();
        let pane = open(&mut cx, held(uniform(20), 1), 900., 10.);
        let states = RowStates {
            listing: 1,
            cursor: Some(4),
            focused: true,
            selected: Some(Rc::from(vec![1, 3, 5])),
            hovered: Some(7),
            hover_wash,
            renaming: Some(2),
            drop_target: Some(8),
            ..Default::default()
        };
        pane.update(&mut cx, |view| view.states = states);
        pane.draw(&mut cx);
        {
            let state = pane.list.borrow();
            let of = |index: usize| {
                state
                    .visible()
                    .iter()
                    .find(|shown| shown.index == index)
                    .unwrap()
                    .state
            };
            assert!(
                of(4).cursor
                    && of(1).selected
                    && of(3).selected
                    && of(5).selected
                    && !of(2).selected
            );
            assert!(of(7).hovered && of(2).renaming && of(8).drop_target);
            assert!(
                of(1).lifted()
                    && of(4).lifted()
                    && of(7).lifted() == hover_wash
                    && of(8).lifted()
                    && !of(2).lifted()
                    && !of(0).lifted()
            );
        }
        // A new listing renumbers the rows: the old snapshot marks no row but the cursor, so no rename or mark lands on a stranger.
        pane.update(&mut cx, |view| view.held = held(uniform(20), 2));
        pane.draw(&mut cx);
        let state = pane.list.borrow();
        for shown in state.visible() {
            let s = shown.state;
            assert!(
                !s.selected && !s.hovered && !s.hover_lit && !s.renaming && !s.drop_target,
                "row {} kept a state from the last listing",
                shown.index
            );
        }
        assert!(state.visible().iter().any(|shown| shown.state.cursor));
    }
}
