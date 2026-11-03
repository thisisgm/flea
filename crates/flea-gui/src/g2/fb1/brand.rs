//! FleaMark.qml's draw-on path, independent of the Omi loading animation.
const SPIRAL: [(f32, f32); 10] = [
    (21., 21.),
    (3., 21.),
    (3., 3.),
    (21., 3.),
    (21., 17.),
    (7., 17.),
    (7., 7.),
    (17., 7.),
    (17., 13.),
    (11., 13.),
];
/// Distance along the centreline to each vertex; the whole spiral is 114 units.
const AT: [f32; 10] = [0., 18., 36., 54., 68., 82., 92., 102., 108., 114.];
pub(crate) const GRID: f32 = 24.;
const LENGTH: f32 = 114.;
const HALF: f32 = 1.;
const QUANTUM: f32 = 256.;

pub(super) fn drawn(length: f32, unit: f32, mut emit: impl FnMut([f32; 4], f32)) {
    run(0., length.clamp(0., LENGTH), unit, &mut emit);
}

/// Spinner.qml's crawl: a 30 on, 27 off dash in stroke units, so one 60 unit dash per 114 unit period, `phase` stroke units along.
pub(crate) fn dashed(phase: f32, unit: f32, mut emit: impl FnMut([f32; 4], f32)) {
    let start = (2. * phase).rem_euclid(LENGTH);
    let end = start + 60.;
    run(start, end.min(LENGTH), unit, &mut emit);
    if end > LENGTH {
        run(0., end - LENGTH, unit, &mut emit);
    }
}

// Square caps and miter corners are one more half stroke at each end; a run carried over a corner starts past the square its predecessor drew.
fn run(from: f32, to: f32, unit: f32, emit: &mut impl FnMut([f32; 4], f32)) {
    for at in 0..SPIRAL.len() - 1 {
        let (near, far) = (from.max(AT[at]), to.min(AT[at + 1]));
        if near >= far {
            continue;
        }
        let low = near - AT[at] + if from < AT[at] { HALF } else { -HALF };
        let high = far - AT[at] + HALF;
        let ((ax, ay), (bx, by)) = (SPIRAL[at], SPIRAL[at + 1]);
        let length = AT[at + 1] - AT[at];
        let (dx, dy) = ((bx - ax) / length, (by - ay) / length);
        let (px0, py0, px1, py1) = (ax + dx * low, ay + dy * low, ax + dx * high, ay + dy * high);
        let (wide, tall) = (HALF * dy.abs(), HALF * dx.abs());
        let rect = [
            (px0.min(px1) - wide) * unit,
            (py0.min(py1) - tall) * unit,
            (px0.max(px1) + wide) * unit,
            (py0.max(py1) + tall) * unit,
        ];
        cover(rect, emit);
    }
}

fn cover([x0, y0, x1, y1]: [f32; 4], emit: &mut impl FnMut([f32; 4], f32)) {
    for (top, bottom, down) in spans(y0, y1).into_iter().flatten() {
        for (left, right, across) in spans(x0, x1).into_iter().flatten() {
            emit([left, top, right, bottom], down * across);
        }
    }
}

// Whole device pixels in one span, plus a partly covered pixel at either end with its coverage.
fn spans(low: f32, high: f32) -> [Option<(f32, f32, f32)>; 3] {
    let (low, high) = (
        (low * QUANTUM).round() / QUANTUM,
        (high * QUANTUM).round() / QUANTUM,
    );
    let (first, last) = (low.ceil(), high.floor());
    if first > last {
        return [Some((last, last + 1., high - low)), None, None];
    }
    [
        (first > low).then_some((first - 1., first, first - low)),
        (last > first).then_some((first, last, 1.)),
        (high > last).then_some((last, last + 1., high - last)),
    ]
}
