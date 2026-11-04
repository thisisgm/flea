//! Review g1-08-r1-2: GridTile.qml nameTip, the whole name under a hovered tile whose caption is elided, and nowhere else.
use super::frames::{frame, held, open, real_cx, rows, state, with};
use flea_gui::{
    grid::{element::DropKind, FocusSnapshot},
    ipc::grid::GridAdapter,
};
use std::rc::Rc;

const LONG: &str = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";

fn names() -> Vec<flea::protocol::RowFacts> {
    let mut names = rows(12);
    names[0].name = LONG.into();
    names[1].name = "my-vacation-1.jpeg".into();
    names[2].name = "IMG_4121.jpg".into();
    names
}

#[test]
fn a_hovered_elided_caption_shows_the_whole_name_under_its_label() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(names()));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| host.view.hovered = Some(0));
    frame(&mut cx, window);
    let view = with(&mut cx, window, |host| host.view.clone());
    let state = grid.borrow();
    let tile = state.drawn_row(0).unwrap();
    assert!(tile.elided(), "the long name is elided");
    let (listing, text, rect) = state.tip().expect("the hovered elided tile shows its tip");
    assert_eq!((listing, text), (0, LONG), "the tip holds the whole name");
    let metrics = state.layout.unwrap().metrics;
    assert_eq!(
        rect.y,
        tile.caption.y + tile.painted.height + metrics.hairline,
        "one hairline under the label"
    );
    assert!(
        rect.width <= 2. * metrics.min_cell,
        "never wider than two minimum cells: {}",
        rect.width
    );
    assert!(
        (rect.x + rect.width / 2. - (tile.cell.x + tile.cell.width / 2.)).abs() <= 0.5,
        "centred on its tile"
    );
    assert!(
        rect.height > metrics.gap,
        "the name lays at least one line inside a gap of padding"
    );
    let adapter = GridAdapter {
        view: &view,
        state: &state,
    };
    assert!(
        adapter
            .caption_state(0)
            .contains(&format!("\"tip\":{{\"text\":\"{LONG}\"")),
        "{}",
        adapter.caption_state(0)
    );
    assert!(
        adapter.caption_state(1).ends_with("\"tip\":null}"),
        "another row reads no tip"
    );
}

// One no-show state applied to a fresh host.
type Set = fn(&mut super::frames::Host);

#[test]
fn no_tip_on_a_whole_caption_without_hover_while_editing_on_a_drop_target_or_under_a_band() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(names()));
    let grid = state(&mut cx, window);
    let cases: [(&str, Set); 6] = [
        ("a whole wrapped caption", |host| {
            host.view.hovered = Some(1)
        }),
        ("a one-line caption", |host| host.view.hovered = Some(2)),
        ("no hover", |host| host.view.hovered = None),
        ("an editing tile", |host| {
            host.view.hovered = Some(0);
            host.overlay.renaming = Some(0);
        }),
        ("a drop target", |host| {
            host.view.hovered = Some(0);
            host.overlay.drop = Some((0, DropKind::Move));
        }),
        ("a sweeping band", |host| {
            host.view.hovered = Some(0);
            host.focus = Rc::new(FocusSnapshot {
                band: true,
                ..FocusSnapshot::default()
            });
        }),
    ];
    for (case, set) in cases {
        with(&mut cx, window, |host| {
            host.overlay = Default::default();
            host.focus = Rc::new(FocusSnapshot::default());
            set(host);
        });
        frame(&mut cx, window);
        assert!(grid.borrow().tip().is_none(), "no tip on {case}");
    }
    with(&mut cx, window, |host| {
        host.overlay = Default::default();
        host.focus = Rc::new(FocusSnapshot::default());
        host.view.hovered = Some(0);
    });
    frame(&mut cx, window);
    assert!(
        grid.borrow().tip().is_some(),
        "positive control: the same tile hovered at rest shows it"
    );
}
