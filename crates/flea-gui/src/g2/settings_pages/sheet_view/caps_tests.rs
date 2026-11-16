//! Legend sizes: text size 14 draws the board's sizes, and a larger text size scales every size and floor with it.
//! Cap geometry: one gap between neighbours at every unit, and ISO Enter's two pieces joined as one key.
use super::{join_areas, legend_sizes, row_top, span, CAP_GAP};
use flea::keys::physical::Part;

fn near(got: (f32, f32), want: (f32, f32)) -> bool {
    (got.0 - want.0).abs() < 1e-3 && (got.1 - want.1).abs() < 1e-3
}

#[test]
fn text_size_14_keeps_the_boards_legends() {
    // A letter at a third of the unit, a name at a quarter floored at 7, a shifted legend at a fifth floored at 8.
    assert!(near(legend_sizes("A", 1., 40., 1.), (12., 8.8)));
    assert!(near(legend_sizes("A", 1., 28., 1.), (8.4, 8.)));
    assert!(near(legend_sizes("TAB", 1.5, 40., 1.), (9.6, 8.8)));
    // The smallest unit squeezes a long name to its floor.
    assert!(near(legend_sizes("CAPS", 1.75, 28., 1.), (7., 8.)));
}

#[test]
fn iso_enters_pieces_share_one_right_edge_at_every_unit() {
    // A unit that is no multiple of four put the foot's quarter-unit edges on half pixels, a 1px jog at the join.
    let caps = flea::keys::physical::caps(true);
    let piece = |part| caps.iter().find(|cap| cap.part == part).unwrap();
    let (top, foot) = (piece(Part::EnterTop), piece(Part::EnterFoot));
    for unit in 28..=56 {
        let unit = unit as f32;
        let (top_left, top_w) = span(top.x, top.w, unit);
        let (foot_left, foot_w) = span(foot.x, foot.w, unit);
        assert_eq!(top_left + top_w, foot_left + foot_w, "unit {unit}");
        assert!(foot_left > top_left, "a step at unit {unit}");
    }
}

#[test]
fn caps_keep_one_gap_between_neighbours_at_every_unit() {
    let caps = flea::keys::physical::caps(true);
    for unit in 28..=56 {
        let unit = unit as f32;
        for cap in &caps {
            let (left, wide) = span(cap.x, cap.w, unit);
            let next = caps
                .iter()
                .find(|next| next.row == cap.row && (next.x - (cap.x + cap.w)).abs() < 1e-3);
            if let Some(next) = next {
                let (next_left, _) = span(next.x, next.w, unit);
                assert_eq!(
                    next_left - (left + wide),
                    CAP_GAP,
                    "{} at unit {unit}",
                    cap.id
                );
            }
        }
    }
}

#[test]
fn iso_enters_join_bridges_the_row_gap_inside_the_step_line() {
    // The top piece is a whole cap of its row, so its bottom edge, the step line, lies a gap above the foot's row.
    let caps = flea::keys::physical::caps(true);
    let foot = caps.iter().find(|cap| cap.part == Part::EnterFoot).unwrap();
    for unit in [28., 38., 39., 40., 56.] {
        for width in [1., 1.5] {
            let ((nx, ny, nw, nh), (ax, ay, aw, ah)) = join_areas(foot, unit, width);
            let (left, wide) = span(foot.x, foot.w, unit);
            let step = row_top(foot.row - 1, unit) + unit - CAP_GAP;
            let foot_top = row_top(foot.row, unit);
            assert_eq!(foot_top - step, CAP_GAP, "unit {unit}");
            // The neck carries the foot's sides from within the step line into the foot, so no edge pokes past either.
            assert_eq!((nx, nw), (left, wide), "unit {unit}");
            assert!(ny >= step - width && ny < step, "neck top at unit {unit}");
            assert!(ny + nh > foot_top, "neck foot at unit {unit}");
            // The patch covers the top's bottom edge between the foot's sides, and only there.
            assert_eq!((ax, aw), (left + width, wide - 2. * width), "unit {unit}");
            assert!(ay < step - width && ay + ah > step, "patch at unit {unit}");
        }
    }
}

#[test]
fn a_larger_text_size_scales_the_legends_and_their_floors() {
    // Text size 20's caption is 17: the unit grows to 56, and every legend with it.
    let scale = 17. / 12.;
    let (letter, shifted) = legend_sizes("A", 1., 56., scale);
    assert!((letter - 16.8).abs() < 1e-3, "a letter at {letter}");
    assert!(
        (shifted - 12.32).abs() < 1e-3,
        "a shifted legend at {shifted}"
    );
    // The smallest unit at that text size keeps its floors in step: never the 7 and 8 of text size 14.
    let (name, shifted) = legend_sizes("CAPS", 1.75, 40., scale);
    assert!((name - 7. * scale).abs() < 1e-3, "a name at {name}");
    assert!(
        (shifted - 8. * scale).abs() < 1e-3,
        "a shifted legend at {shifted}"
    );
}
