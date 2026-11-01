//! EY-349: the copy mark sits one gap after the drawn name, never at the name column's right edge.
use super::*;
use gpui::{point, size, LineLayout};

fn shown(width: f32) -> Shown {
    let prepared = Prepared {
        name: GlyphRun::new(Arc::new(LineLayout {
            width: px(width),
            ..LineLayout::default()
        })),
        ..Prepared::default()
    };
    Shown {
        index: 0,
        view: 0,
        prepared: Some(Rc::new(prepared)),
        clip: Some(ClipMark::Copy),
        state: RowState::default(),
        picture: None,
    }
}

#[test]
fn ey349_copy_mark_follows_the_drawn_name() {
    let metrics = Metrics::at(px(14.));
    let column = Bounds::new(point(px(100.), px(0.)), size(px(600.), px(28.)));
    let mark = shown(60.).clip_bounds(column, &metrics).unwrap();
    assert_eq!(mark.origin.x, px(100. + 60. + metrics.gap));
    // A name that fills its slot keeps the mark inside the column, in the reserved strip.
    let mark = shown(900.).clip_bounds(column, &metrics).unwrap();
    assert_eq!(mark.origin.x, px(100. + 600. - metrics.clip));
}
