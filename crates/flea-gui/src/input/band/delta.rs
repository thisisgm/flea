//! Visit only the cell strips entering or leaving a marquee rectangle.
use super::{Band, Frame};

type Cells = (usize, usize, usize, usize);

fn difference(box_: Option<Cells>, other: Option<Cells>, mut visit: impl FnMut(Cells)) {
    let Some((l, r, t, b)) = box_ else { return };
    if let Some((ol, or, ot, ob)) =
        other.filter(|&(ol, or, ot, ob)| ol < r && or > l && ot < b && ob > t)
    {
        let (top, bottom) = (t.max(ot), b.min(ob));
        for strip in [
            (l, r, t, top),
            (l, r.min(ol), top, bottom),
            (l.max(or), r, top, bottom),
            (l, r, bottom, b),
        ] {
            if strip.0 < strip.1 && strip.2 < strip.3 {
                visit(strip);
            }
        }
    } else {
        visit((l, r, t, b));
    }
}

pub(super) fn apply(
    frame: Frame,
    band: &mut Band,
    selected: &mut Vec<usize>,
    shown: Option<&[usize]>,
    cells: Option<Cells>,
) -> usize {
    let mut touched = 0;
    let index = |view| {
        shown
            .and_then(|shown| shown.get(view).copied())
            .unwrap_or(view)
    };
    let columns = frame.columns.max(1);
    difference(band.cells, cells, |(l, r, t, b)| {
        for row in t..b {
            for view in row * columns + l..(row * columns + r).min(frame.total) {
                touched += 1;
                let row = index(view);
                if band.additive && band.before.binary_search(&row).is_ok() {
                    continue;
                }
                if let Ok(at) = selected.binary_search(&row) {
                    selected.remove(at);
                }
            }
        }
    });
    let mut entered = false;
    difference(cells, band.cells, |(l, r, t, b)| {
        for row in t..b {
            for view in row * columns + l..(row * columns + r).min(frame.total) {
                touched += 1;
                entered = true;
                let row = index(view);
                if let Err(at) = selected.binary_search(&row) {
                    selected.insert(at, row);
                }
            }
        }
    });
    if let (true, Some((l, r, t, b))) = (entered, cells) {
        let row = if band.end.1 < band.anchor.1 { t } else { b - 1 };
        let column = if band.end.0 < band.anchor.0 { l } else { r - 1 };
        band.last = Some(index((row * columns + column).min(frame.total - 1)));
    }
    touched
}
