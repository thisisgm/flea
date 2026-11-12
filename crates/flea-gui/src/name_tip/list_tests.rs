//! The shared List painter supplies the same geometry and row inks in a window or chooser.
use super::list_candidate;
use crate::list::{bench, pane::Held, ListElement, ListState, RowStates};
use gpui::{div, point, prelude::*, px, size, Context, IntoElement, Render, Window};
use std::{cell::RefCell, rc::Rc};

struct Fixture {
    state: Rc<RefCell<ListState>>,
    held: Rc<Held>,
    marks: RowStates,
}

#[test]
fn name_tip_geometry_is_body_14_with_seven_pixel_padding_in_each_list_host() {
    for (host, panes) in [("list", 1), ("dual", 2), ("picker", 1)] {
        let mut cx = bench::real_cx();
        let mut rows = bench::fixture("2k");
        rows.truncate(1);
        rows[0].name = "riverside-community-garden-committee-meeting-notes-2025-06-02-spring-review-final-approved.md".into();
        rows[0].dir = false;
        rows[0].mode = 0o100644;
        let held = bench::held(rows);
        let states: Vec<_> = (0..panes)
            .map(|_| {
                let mut state = ListState::new(bench::bench_typeface());
                state.view.dual = host == "dual";
                state.view.picker = (host == "picker").then_some(true);
                state.set_words(crate::row::format::Words::from_catalog(&bench::english()));
                Rc::new(RefCell::new(state))
            })
            .collect();
        let window = cx.open_window(
            size(px(if panes == 2 { 1100. } else { 900. }), px(500.)),
            |_, _| GeometryFixture {
                states: states.clone(),
                held: held.clone(),
            },
        );
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        for (side, state) in states.iter().enumerate() {
            let state = state.borrow();
            let drawn = &state.drawn;
            let at = point(
                drawn.bounds.left() + px(drawn.layout.name.x + 10.),
                drawn.bounds.top() + px(drawn.metrics.row / 2.),
            );
            let right = drawn.bounds.right() - px(14.);
            let tip = list_candidate(&state, at, held.listing, right, side, side == 1).unwrap();
            assert_eq!(tip.size, px(14.), "{host} side {side} body");
            assert_eq!(tip.pad, px(7.), "{host} side {side} padding");
            assert_eq!(tip.height, px(23.), "{host} side {side} plate");
            assert_eq!(tip.text_x, drawn.bounds.left() + px(drawn.layout.name.x));
            assert_eq!(tip.row.right(), right);
            if side == 1 {
                assert!(drawn.bounds.left() >= px(550.));
            }
        }
    }
}

struct GeometryFixture {
    states: Vec<Rc<RefCell<ListState>>>,
    held: Rc<Held>,
}

impl Render for GeometryFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .children(self.states.iter().map(|state| {
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(ListElement::with_states(
                        self.held.clone(),
                        0.,
                        RowStates::cursor_only(self.held.listing, 0),
                        state.clone(),
                    ))
            }))
    }
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(ListElement::with_states(
            self.held.clone(),
            0.,
            self.marks.clone(),
            self.state.clone(),
        ))
    }
}

#[test]
fn name_tip_list_rest_cursor_marked_picker_and_fitting_names_follow_painted_rows() {
    for picker in [false, true] {
        for (cursor, selected) in [(0, false), (1, false), (0, true)] {
            let mut cx = bench::real_cx();
            let mut rows = bench::fixture("2k");
            rows.truncate(2);
            rows[0].name = "fits.txt".into();
            rows[1].name = "riverside-community-garden-committee-meeting-notes-2025-06-02-spring-review-final-approved.md".into();
            rows[1].dir = false;
            rows[1].mode = 0o100644;
            let held = bench::held(rows);
            let mut state = ListState::new(bench::bench_typeface());
            state.view.picker = picker.then_some(true);
            state.set_words(crate::row::format::Words::from_catalog(&bench::english()));
            let state = Rc::new(RefCell::new(state));
            let window = cx.open_window(size(px(900.), px(500.)), |_, _| Fixture {
                state: state.clone(),
                held: held.clone(),
                marks: RowStates {
                    listing: held.listing,
                    cursor: Some(cursor),
                    focused: true,
                    selected: selected.then(|| Rc::from([1])),
                    ..Default::default()
                },
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .unwrap();
            let state = state.borrow();
            let drawn = &state.drawn;
            let at = |index: f32| {
                point(
                    drawn.bounds.left() + px(drawn.layout.name.x + 10.),
                    drawn.bounds.top() + px((index + 0.5) * drawn.metrics.row),
                )
            };
            assert!(list_candidate(&state, at(0.), held.listing, px(886.), 1, false).is_none());
            let tip = list_candidate(&state, at(1.), held.listing, px(886.), 1, false).unwrap();
            assert_eq!(tip.text_x, drawn.bounds.left() + px(drawn.layout.name.x));
            assert_eq!(tip.height, px(drawn.metrics.slot));
            assert_eq!(tip.row.right(), px(886.));
            assert_eq!(tip.framed, cursor != 1 && !selected);
            assert_eq!(tip.ink, drawn.palette.unwrap().foreground);
            assert!(list_candidate(&state, at(1.), held.listing + 1, px(886.), 1, false).is_none());
        }
    }
}
