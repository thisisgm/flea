//! Finder rule 9 in Columns: every column hairline drags the track to its left, the third gives the room, and a double click restores it.
use super::fixture::{row, Pane, Producers, SETTINGS};
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::Area;

const WIDTH: f32 = 1200.;
const HEIGHT: f32 = 600.;

fn studio() -> Area {
    let mut area = Area::new(WIDTH, HEIGHT, None, 0., SETTINGS);
    let pane = Pane::at(
        "/probe/studio/deep/er",
        vec![row("a.txt", false), row("sub", true)],
    );
    let mut producers = Producers::default();
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    area
}

fn width(area: &Area, slot: Slot) -> f32 {
    area.layout.track(slot).unwrap().width
}

#[test]
fn finder_a_column_hairline_is_hot_and_drags_only_its_own_track() {
    let mut area = studio();
    assert_eq!(
        (
            width(&area, Slot::Parent),
            width(&area, Slot::Active),
            width(&area, Slot::Child)
        ),
        (400., 400., 400.)
    );
    // The Parent track's hairline is its last pixel column, x 399.
    area.edge_hover(399.);
    assert!(area.host.edge.hovered, "the column hairline is not hot");
    assert_eq!(
        area.edge_paint().mark,
        Some((391.5, ((HEIGHT - 16.) / 2.).round(), 16.)),
        "the mark is not centred on the column hairline and the middle row"
    );
    area.edge_press(399.);
    area.edge_move(439.);
    assert_eq!(
        width(&area, Slot::Parent),
        440.,
        "the dragged track did not follow"
    );
    assert_eq!(
        width(&area, Slot::Active),
        400.,
        "the track right of the line changed width"
    );
    assert_eq!(
        width(&area, Slot::Child),
        360.,
        "the third did not give the room"
    );
    assert_eq!(
        area.layout.track(Slot::Active).unwrap().x,
        440.,
        "the next track did not slide"
    );
    let _ = area.edge_release();
    assert_eq!(
        width(&area, Slot::Parent),
        440.,
        "the release lost the width"
    );
    assert_eq!(
        area.stored.last(),
        Some(&30),
        "the third's share was not stored"
    );
    // A double click on the same hairline restores that track's default width.
    area.edge_hover(439.);
    let _ = area.edge_restore();
    assert_eq!(
        (
            width(&area, Slot::Parent),
            width(&area, Slot::Active),
            width(&area, Slot::Child)
        ),
        (400., 400., 400.)
    );
}

#[test]
fn finder_the_preview_edge_keeps_the_dragged_tracks_proportions() {
    let mut area = studio();
    area.edge_hover(399.);
    area.edge_press(399.);
    area.edge_move(499.);
    let _ = area.edge_release();
    assert_eq!(
        (width(&area, Slot::Parent), width(&area, Slot::Active)),
        (500., 400.)
    );
    // The preview edge, now at x 900, moves the room left of the third; both tracks keep their 5 to 4.
    area.edge_hover(899.);
    area.edge_press(899.);
    area.edge_move(779.);
    let _ = area.edge_release();
    assert_eq!(width(&area, Slot::Child), 420.);
    assert_eq!(
        (width(&area, Slot::Parent), width(&area, Slot::Active)),
        (433., 347.)
    );
}

#[test]
fn finder_preview_resize_keeps_every_weighted_track_at_its_minimum() {
    let mut area = studio();
    area.edge_hover(399.);
    area.edge_press(399.);
    area.edge_move(299.);
    let _ = area.edge_release();
    let edge = area.layout.edge() - 1.;
    area.edge_hover(edge);
    area.edge_press(edge);
    area.edge_move(0.);
    let _ = area.edge_release();
    for track in area.layout.tracks() {
        assert!(
            track.width >= area.layout.min_column,
            "{:?} width {} fell below {}",
            track.slot,
            track.width,
            area.layout.min_column
        );
    }
}

#[test]
fn finder_weighted_tracks_fill_the_room_for_every_column_limit() {
    for (width, count) in [(1200., 3), (1800., 4), (2400., 5)] {
        let mut area = Area::new(width, HEIGHT, Some(count as f64), 0., SETTINGS);
        area.set_weights((1..count).map(|weight| weight as f32).collect());
        for share in [0., 80.] {
            area.set_share(share);
            let tracks = area.layout.tracks().collect::<Vec<_>>();
            assert_eq!(tracks.len(), count);
            assert_eq!(tracks.iter().map(|track| track.width).sum::<f32>(), width);
            for track in tracks {
                assert!(
                    track.width >= area.layout.min_column,
                    "count {count}, share {share}: {:?} width {} below {}",
                    track.slot,
                    track.width,
                    area.layout.min_column
                );
            }
        }
    }
}

#[test]
fn finder_column_reset_restores_default_when_preview_is_at_minimum() {
    let mut area = studio();
    area.set_weights(vec![1., 2.]);
    area.set_share(25.);
    assert_eq!(width(&area, Slot::Parent), area.layout.min_column);
    assert_eq!(width(&area, Slot::Child), area.layout.min_column);
    let edge = width(&area, Slot::Parent) - 1.;
    area.edge_hover(edge);
    let _ = area.edge_restore();
    assert_eq!(width(&area, Slot::Parent), WIDTH / 3.);
    for track in area.layout.tracks() {
        assert!(track.width >= area.layout.min_column);
    }
}

#[test]
fn finder_column_reset_keeps_default_when_rounding_reaches_the_next_track_minimum() {
    let mut area = studio();
    area.set_weights(vec![5., 3.]);
    area.set_share(33.);
    let edge = width(&area, Slot::Parent) - 1.;
    area.edge_hover(edge);
    let _ = area.edge_restore();
    assert_eq!(width(&area, Slot::Parent), WIDTH / 3.);
    assert!(width(&area, Slot::Active) >= area.layout.min_column);
}

#[test]
fn finder_column_reset_can_borrow_room_at_every_column_limit() {
    for (width, count) in [(1200., 3), (1800., 4), (2400., 5)] {
        let mut area = Area::new(width, HEIGHT, Some(count as f64), 100., SETTINGS);
        area.set_weights((1..count).map(|weight| weight as f32).collect());
        let edge = area.layout.widths[0] - 1.;
        area.edge_hover(edge);
        let _ = area.edge_restore();
        assert_eq!(area.layout.widths[0], (width / count as f32).floor());
        assert!(area
            .layout
            .tracks()
            .all(|track| track.width >= area.layout.min_column));
    }
}

#[test]
fn finder_column_reset_rounding_can_borrow_past_a_minimum_next_track() {
    for at in 0..3 {
        let mut area = Area::new(2400., HEIGHT, Some(5.), 12., SETTINGS);
        area.set_weights(vec![2., 2., 5., 5.]);
        assert_eq!(area.layout.widths, vec![300., 300., 750., 750.]);
        assert_eq!(area.layout.third_width, 300.);
        area.edge_hover(area.layout.widths.iter().take(at + 1).sum::<f32>() - 1.);
        let _ = area.edge_restore();
        assert_eq!(area.layout.widths[at], 480., "column {at}");
        assert!(area
            .layout
            .tracks()
            .all(|track| track.width >= area.layout.min_column));
        assert_eq!(
            area.layout.tracks().map(|track| track.width).sum::<f32>(),
            2400.
        );
        let mut reopened = Area::new(2400., HEIGHT, Some(5.), area.share, SETTINGS);
        reopened.set_weights(area.weights.clone());
        assert_eq!(reopened.layout.widths, area.layout.widths);
    }
}

#[test]
fn finder_four_column_reset_keeps_default_through_percent_rounding() {
    let mut area = Area::new(1800., HEIGHT, Some(4.), 34., SETTINGS);
    area.set_weights(vec![444., 300., 444.]);
    assert_eq!(area.layout.widths, vec![444., 300., 444.]);
    area.edge_hover(443.);
    let _ = area.edge_restore();
    assert_eq!(area.layout.widths, vec![450., 300., 438.]);
    let mut reopened = Area::new(1800., HEIGHT, Some(4.), area.share, SETTINGS);
    reopened.set_weights(area.weights.clone());
    assert_eq!(reopened.layout, area.layout);
}
