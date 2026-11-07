//! The Jobs card's spiral is Spinner.qml's: 16 px, muted frame ink, still 1 s then a linear 1.6 s crawl, 55% when waiting.
use super::draw_fixture::open_card;
use crate::feedback::spinner::{spiral_phase, spiral_seconds};
use flea::ui_logic_op1::card;
use gpui::{point, px, Bounds, Hsla, Pixels, ScaledPixels, TestAppContext, VisualTestContext};
use std::time::Duration;

const FRAME: Duration = Duration::from_millis(16);

fn job(id: u64, state: &str) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"copy","head":"Copying 10 of 18 to NAS","file":"DSC_4944.jpg","done":9,"total":18,"bytes":5300000,"totalBytes":9700000,"itemBytes":458300,"rate":1100000,"left":4,"state":"{state}","reason":"waiting for the copy to NAS","output":[],"undo":false,"cancelled":false}},"unfinished":1}}"#
    )
}

// Spinner.qml's dash at `seconds` in a 16 px slot, as device-pixel bounds and inks.
fn expected(
    slot: Bounds<Pixels>,
    scale: f32,
    seconds: f32,
    ink: Hsla,
) -> Vec<(Bounds<ScaledPixels>, Hsla)> {
    let unit = card::JOBS_MARK * scale / crate::g2::fb1::brand::GRID;
    let (left, top) = (
        (f32::from(slot.origin.x) * scale).round(),
        (f32::from(slot.origin.y) * scale).round(),
    );
    let mut out = Vec::new();
    crate::g2::fb1::brand::dashed(spiral_phase(seconds), unit, |[x0, y0, x1, y1], coverage| {
        let logical = Bounds::from_corners(
            point(px((left + x0) / scale), px((top + y0) / scale)),
            point(px((left + x1) / scale), px((top + y1) / scale)),
        );
        out.push((logical.scale(scale), ink.opacity(coverage)));
    });
    out
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 2e-3
}

// Every quad painted inside the mark is one expected dash quad, and every expected quad is painted.
fn painted_matches(
    cx: &mut VisualTestContext,
    selector: &'static str,
    seconds: f32,
    ink: Hsla,
) -> bool {
    let slot = cx.debug_bounds(selector).unwrap();
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let square = slot.scale(scale);
        let mut quads: Vec<_> = window
            .painted_quads()
            .into_iter()
            .filter(|quad| quad.bounds.is_contained_within(&square))
            .collect();
        for (bounds, ink) in expected(slot, scale, seconds, ink) {
            let Some(at) = quads.iter().position(|quad| {
                let paint = quad.background.as_solid();
                near(quad.bounds.origin.x.0, bounds.origin.x.0)
                    && near(quad.bounds.origin.y.0, bounds.origin.y.0)
                    && near(quad.bounds.size.width.0, bounds.size.width.0)
                    && near(quad.bounds.size.height.0, bounds.size.height.0)
                    && paint.is_some_and(|paint| {
                        (paint.h, paint.s, paint.l) == (ink.h, ink.s, ink.l) && near(paint.a, ink.a)
                    })
            }) else {
                return false;
            };
            quads.remove(at);
        }
        quads.is_empty()
    })
}

fn frames(cx: &mut VisualTestContext, count: usize) -> usize {
    (0..count)
        .map(|_| {
            cx.executor().advance_clock(FRAME);
            cx.update(|window, cx| window.simulate_next_frame(cx))
        })
        .sum()
}

#[test]
fn the_crawl_clock_is_spinner_qml() {
    // Still for its first second, then one 57 stroke unit period per 1.6 s, linear, looping on the period.
    assert_eq!(spiral_phase(0.5), 0.);
    assert!(near(spiral_phase(1.4), 14.25));
    assert!(near(spiral_phase(1.8), 28.5));
    assert!(near(spiral_phase(2.6 + 1.6), 0.) || near(spiral_phase(2.6 + 1.6), 57.));
    for phase in [0., 13.5, 27., 56.] {
        assert!(near(spiral_phase(spiral_seconds(phase)), phase), "{phase}");
    }
}

#[gpui::test]
fn the_copy_card_draws_spinner_qml_in_the_muted_frame(cx: &mut TestAppContext) {
    let (overlay, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.line(&cat, &job(12, "running"), 0);
    });
    let mark = vcx.debug_bounds("op1-job-12-mark").unwrap();
    assert_eq!(
        (f32::from(mark.size.width), f32::from(mark.size.height)),
        (card::JOBS_MARK, card::JOBS_MARK)
    );
    let (muted_frame, accent) =
        overlay.read_with(vcx, |o, _| (o.palette.muted_frame, o.palette.accent));
    assert_ne!(muted_frame, accent);
    // At rest on its first frame, the dash on the outer turns; Omi's grid or the accent ink fails here.
    assert!(painted_matches(vcx, "op1-job-12-mark", 0., muted_frame));
    assert!(!painted_matches(vcx, "op1-job-12-mark", 0., accent));
    // Still through the first second.
    assert_eq!(frames(vcx, 62), 62);
    assert!(painted_matches(vcx, "op1-job-12-mark", 0., muted_frame));
    // 1.8 s in, half a period along: the 0.8 s of crawl moved the dash 57 grid units.
    vcx.executor()
        .advance_clock(Duration::from_millis(1800 - 62 * 16 - 16));
    assert_eq!(frames(vcx, 1), 1);
    assert!(painted_matches(vcx, "op1-job-12-mark", 1.8, muted_frame));
    assert!(!painted_matches(vcx, "op1-job-12-mark", 1., muted_frame));
}

#[gpui::test]
fn a_waiting_job_holds_the_board_frame_at_55_percent(cx: &mut TestAppContext) {
    let (overlay, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.line(&cat, &job(13, "waiting"), 0);
    });
    let ink = overlay.read_with(vcx, |o, _| {
        o.palette.muted_frame.opacity(card::JOBS_WAITING_ALPHA)
    });
    let board = spiral_seconds(27.);
    assert!(painted_matches(vcx, "op1-job-13-mark", board, ink));
    // A still mark asks for no frame, so the card idles.
    vcx.executor().advance_clock(FRAME);
    vcx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(frames(vcx, 64), 0);
    assert!(painted_matches(vcx, "op1-job-13-mark", board, ink));
}

#[gpui::test]
fn the_card_spiral_keeps_its_16_px_dash_at_fractional_scales(cx: &mut TestAppContext) {
    let (overlay, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.line(&cat, &job(15, "waiting"), 0);
    });
    let ink = overlay.read_with(vcx, |o, _| {
        o.palette.muted_frame.opacity(card::JOBS_WAITING_ALPHA)
    });
    let handle = vcx.update(|window, _| window.window_handle());
    // Omi's floor laid the slot out at 22 device px, 17.6 px at 1.25; the dash must fill the 16 px slot at every scale.
    for scale in [1.25, 1.5, 2.] {
        vcx.simulate_window_scale_factor_change(handle, scale);
        vcx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        let mark = vcx.debug_bounds("op1-job-15-mark").unwrap();
        assert_eq!(
            (f32::from(mark.size.width), f32::from(mark.size.height)),
            (card::JOBS_MARK, card::JOBS_MARK),
            "{scale}"
        );
        assert!(
            painted_matches(vcx, "op1-job-15-mark", spiral_seconds(27.), ink),
            "{scale}"
        );
    }
}

#[gpui::test]
fn the_crawl_stops_asking_for_frames_once_the_card_is_gone(cx: &mut TestAppContext) {
    let (overlay, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.line(&cat, &job(14, "running"), 0);
    });
    assert_eq!(frames(vcx, 8), 8);
    overlay.update(vcx, |o, cx| {
        o.fold_jobs(cx);
    });
    vcx.run_until_parked();
    // The callback queued before the fold is delivered; a mark no longer drawn queues no successor.
    vcx.update(|window, cx| window.simulate_next_frame(cx));
    assert!(vcx.debug_bounds("op1-job-14-mark").is_none());
    assert_eq!(frames(vcx, 64), 0);
}
