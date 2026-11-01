use super::{update_band, Band, Frame, PaneInput};
use gpui::{point, px, size, Bounds};
use std::rc::Rc;

fn fixture(columns: usize) -> (Frame, Band, Vec<usize>) {
    let frame = Frame {
        bounds: Bounds::new(point(px(0.), px(0.)), size(px(800.), px(200_000.))),
        row: 20.,
        lane: 14.,
        content: 200_000.,
        scroll: 0.,
        listing: 1,
        total: 10_000,
        columns,
        cell_width: 100.,
        grow: Default::default(),
        cell_origin: (0., 0.),
    };
    let band = Band {
        start: point(px(0.), px(0.)),
        anchor: (0., 0.),
        end: (0., 0.),
        before: vec![9_999],
        cursor: 0,
        active: true,
        additive: false,
        shift: false,
        direction: 0.,
        last: None,
        cells: None,
        basis: None,
        pointer: point(px(90.), px(100_000.)),
    };
    (frame, band, vec![9_999])
}

#[test]
fn vf4_band_a_one_row_update_touches_only_its_strip_on_ten_thousand_rows() {
    let (frame, mut band, mut selected) = fixture(1);
    assert_eq!(update_band(frame, &mut band, &mut selected, None), 5_000);
    assert_eq!(selected.len(), 5_000);
    band.pointer.y += px(20.);
    assert_eq!(update_band(frame, &mut band, &mut selected, None), 1);
    assert_eq!(selected.len(), 5_001);
    assert_eq!(update_band(frame, &mut band, &mut selected, None), 0);
    band.pointer.y -= px(40.);
    assert_eq!(update_band(frame, &mut band, &mut selected, None), 2);
    assert_eq!(selected, (0..4_999).collect::<Vec<_>>());
}

#[test]
fn vf4_band_grid_strips_preserve_additive_marks_and_filtered_indices() {
    let (frame, mut band, mut selected) = fixture(4);
    band.additive = true;
    band.pointer = point(px(190.), px(20_000.));
    let shown: Vec<_> = (0..10_000).rev().collect();
    assert_eq!(
        update_band(frame, &mut band, &mut selected, Some(&shown)),
        2_000
    );
    band.pointer.x += px(100.);
    assert_eq!(
        update_band(frame, &mut band, &mut selected, Some(&shown)),
        1_000
    );
    band.pointer.x -= px(200.);
    assert_eq!(
        update_band(frame, &mut band, &mut selected, Some(&shown)),
        2_000
    );
    assert!(selected.contains(&9_999));
    assert!(selected.windows(2).all(|v| v[0] < v[1]));
    band.pointer.x = px(0.);
    update_band(frame, &mut band, &mut selected, Some(&shown));
    assert_eq!(selected, band.before);
}

#[test]
fn vf4_band_strip_result_matches_a_full_oracle_across_anchor_crossings() {
    let (mut frame, mut band, mut selected) = fixture(4);
    frame.total = 9_999;
    frame.cell_origin = (8., 5.);
    band.anchor = (180., 40_005.);
    band.before = vec![3, 9_998];
    selected.clone_from(&band.before);
    band.additive = true;
    let shown: Vec<_> = (0..frame.total).rev().collect();
    for (x, y) in [
        (390., 60_005.),
        (290., 60_025.),
        (190., 60_025.),
        (90., 30_005.),
        (180., 40_005.),
        (390., 60_005.),
        (0., 80_005.),
    ] {
        band.pointer = point(px(x), px(y));
        update_band(frame, &mut band, &mut selected, Some(&shown));
        let mut expected = band.before.clone();
        if let Some((l, r, t, b)) = band.cells {
            for row in t..b {
                expected.extend_from_slice(
                    &shown[row * frame.columns + l..(row * frame.columns + r).min(frame.total)],
                );
            }
        }
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(selected, expected, "pointer {x}, {y}");
    }
}

// The selection a full rebuild makes: the starting marks when additive, plus every covered cell's row.
fn oracle(band: &Band, frame: Frame, shown: Option<&[usize]>) -> Vec<usize> {
    let mut rows = if band.additive {
        band.before.clone()
    } else {
        Vec::new()
    };
    if let Some((l, r, t, b)) = band.cells {
        for row in t..b {
            for view in row * frame.columns + l..(row * frame.columns + r).min(frame.total) {
                rows.push(shown.map_or(view, |shown| shown[view]));
            }
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows
}

#[test]
fn vf4_r1_band_rebuilds_when_the_grid_reflows_mid_drag() {
    let (mut frame, mut band, mut selected) = fixture(4);
    band.pointer = point(px(190.), px(20_000.));
    update_band(frame, &mut band, &mut selected, None);
    frame.columns = 5;
    band.pointer.x += px(100.);
    update_band(frame, &mut band, &mut selected, None);
    assert_eq!(selected, oracle(&band, frame, None));
}

#[test]
fn vf4_r1_a_new_filter_answer_mid_band_rebuilds_from_the_starting_marks() {
    let (frame, band, mut selected) = fixture(1);
    let mut input = PaneInput {
        shown: Some((0..10_000).collect()),
        band: Some(band),
        ..Default::default()
    };
    let step = |input: &mut PaneInput, selected: &mut Vec<usize>| {
        let PaneInput { band, shown, .. } = input;
        update_band(frame, band.as_mut().unwrap(), selected, shown.as_deref());
    };
    step(&mut input, &mut selected);
    let reversed: Rc<[usize]> = (0..10_000).rev().collect();
    input.set_shown(Some(reversed.clone()));
    input.band.as_mut().unwrap().pointer.y += px(20.);
    step(&mut input, &mut selected);
    let band = input.band.as_ref().unwrap();
    assert_eq!(selected, oracle(band, frame, Some(&reversed)));
}

#[test]
fn vf4_r1_an_additive_band_over_unsorted_marks_keeps_one_sorted_copy() {
    let (frame, mut band, _) = fixture(1);
    band.additive = true;
    band.before = vec![7, 2];
    let mut selected = vec![7, 2];
    band.pointer = point(px(90.), px(70.));
    update_band(frame, &mut band, &mut selected, None);
    assert_eq!(selected, oracle(&band, frame, None));
    assert_eq!(selected, [0, 1, 2, 3, 7]);
}
