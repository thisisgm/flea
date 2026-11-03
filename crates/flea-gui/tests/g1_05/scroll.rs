use super::*;
use flea_gui::input::{
    glided,
    scroll::{self, Axis, Kinetic, Limits, Phase, Sample, Wheel},
    Glide,
};

fn sample(t: f64, x: f64, y: f64) -> Sample {
    Sample { t, x, y }
}

#[test]
fn notch_pixels_and_touchpad_routing_match_scroll_js() {
    assert_eq!(scroll::distance(0., -120., 3., 24., 4.), -288.);
    assert_eq!(scroll::distance(0., 120., 3., 24., 4.), 288.);
    assert_eq!(scroll::distance(0., -240., 3., 24., 4.), -576.);
    assert_eq!(scroll::distance(0., -120., 0., 24., 4.), -288.);
    assert_eq!(scroll::distance(0., -120., -1., 24., 4.), -288.);
    assert_eq!(scroll::distance(-10., 0., 3., 24., 4.), -10.);
    assert_eq!(scroll::distance(-15., -120., 3., 24., 4.), -288.);
    assert_eq!(scroll::distance(15., 120., 3., 24., 4.), 288.);
    assert_eq!(scroll::distance(-2.5, 0., 3., 24., 4.), -2.5);
    assert_eq!(scroll::distance(0., 0., 3., 24., 4.), 0.);
    assert!(scroll::moves_by_pixels(Phase::Update, -15., -120.));
    assert!(!scroll::moves_by_pixels(Phase::None, -15., -120.));
    assert!(scroll::moves_by_pixels(Phase::None, -10., 0.));
    assert!(!scroll::moves_by_pixels(Phase::Update, 0., -120.));
    assert!(!scroll::is_touchpad(Phase::None));
    for phase in [Phase::Begin, Phase::Update, Phase::End] {
        assert!(scroll::is_touchpad(phase));
    }
    assert_eq!(scroll::touch_distance(-10.), -25.);
    assert_eq!(scroll::touch_distance(0.), 0.);
    assert_eq!(
        scroll::touch_distance(scroll::touchpad_raw(30.)),
        25.,
        "GPUI reports a raw 10 px stroke as 30 px"
    );
}

#[test]
fn lift_velocity_tail_and_elastic_edges_match_scroll_js() {
    assert_eq!(scroll::lift_velocity(&[sample(0., -50., 0.)], 500.).0, 0.);
    assert_eq!(
        scroll::lift_velocity(&[sample(0., 0., 0.), sample(50., -100., 0.)], 50.).0,
        -2.
    );
    let frames = [
        sample(0., 0., 0.),
        sample(8., 25., 0.),
        sample(16., 25., 0.),
        sample(24., 25., 0.),
    ];
    assert_eq!(scroll::lift_velocity(&frames, 104.).0, 3.125);
    assert_eq!(
        scroll::lift_velocity(&[sample(100., 25., 0.), sample(100., 25., 0.)], 100.).0,
        3.125
    );
    assert_eq!(
        scroll::lift_velocity(&[sample(0., 500., 0.), sample(90., 500., 0.)], 90.).0,
        6.
    );
    assert_eq!(scroll::tail_total(0.02), 0.);
    assert!(scroll::tail_live(1., 0.));
    assert!(!scroll::tail_live(0.02, 0.));
    let (mut v, mut sum, mut guard) = (2., 0., 0);
    while scroll::tail_live(v, 0.) && guard < 10_000 {
        let (dx, next) = scroll::tail_step(v, 16.7);
        sum += dx;
        v = next;
        guard += 1;
    }
    assert!((sum - scroll::tail_total(2.)).abs() < 1.);
    assert!(
        scroll::tail_travel(2., 500.) > 0.
            && scroll::tail_travel(2., 500.) < scroll::tail_total(2.)
    );
    assert_eq!(scroll::over_resist(0., 400.), 0.);
    let small = scroll::over_resist(100., 400.);
    assert!(small > 0. && small < 100.);
    assert!(scroll::over_resist(200., 400.) > small && scroll::over_resist(100_000., 400.) < 400.);
    assert_eq!(scroll::over_resist(100., 0.), 0.);
    assert_eq!(scroll::return_at(80., 0., 0., 200.), 80.);
    assert_eq!(scroll::return_at(80., 0., 200., 200.), 0.);
    assert!(scroll::moved(100., 388.));
    assert!(!scroll::moved(600., 600.));
    assert!(!scroll::moved(600., 600.005));
    let braked = scroll::over_decel(6., 16.7);
    assert!(braked > 0. && braked < 6. * 0.998f64.powf(16.7));
    assert_eq!(scroll::RETURN_MS, 180.);
    assert_eq!(scroll::out_cubic(0.), 0.);
    assert_eq!(scroll::out_cubic(1.), 1.);
    assert!(
        scroll::out_cubic(0.5) > 0.5,
        "OutCubic leaves the start fast"
    );
}

fn axis(at: f64, max: f64) -> Axis {
    Axis {
        at,
        limits: Limits { min: 0., max },
        size: 400.,
        interactive: true,
    }
}

fn wheel(phase: Phase, pixel: f64, angle: f64) -> Wheel {
    Wheel {
        phase,
        pixel,
        angle,
        lines: 0.,
    }
}

#[test]
fn a_notch_writes_inside_the_bounds_and_is_consumed_only_when_it_moved() {
    let mut kinetic = Kinetic::default();
    let mut view = axis(300., 600.);
    assert!(kinetic.wheel(&mut view, wheel(Phase::None, 0., -120.), 0., false));
    assert_eq!(view.at, 588.);
    assert!(kinetic.wheel(&mut view, wheel(Phase::None, 0., -120.), 10., false));
    assert_eq!(view.at, 600.);
    assert!(
        !kinetic.wheel(&mut view, wheel(Phase::None, 0., -120.), 20., false),
        "an edge notch keeps propagating"
    );
    assert_eq!(view.at, 600.);
    let mut idle = Axis {
        interactive: false,
        ..axis(0., 600.)
    };
    assert!(!kinetic.wheel(&mut idle, wheel(Phase::None, 0., -120.), 30., false));
    assert!(!kinetic.running());
}

#[test]
fn a_touchpad_lift_coasts_on_the_closed_form() {
    let mut kinetic = Kinetic::default();
    let mut view = axis(20_000., 90_000.);
    let mut now = 1000.;
    kinetic.wheel(&mut view, wheel(Phase::Begin, 0., 0.), now, false);
    for _ in 0..12 {
        now += 8.;
        assert!(kinetic.wheel(&mut view, wheel(Phase::Update, -40., 0.), now, false));
    }
    assert_eq!(view.at, 20_000. + 12. * 100.);
    kinetic.wheel(&mut view, wheel(Phase::End, 0., 0.), now, false);
    assert!(kinetic.tail_active());
    let speed = kinetic.velocity().abs();
    assert!(
        (speed - scroll::MAX_V_PX_PER_MS).abs() < 1e-9,
        "a fast stroke lifts at the cap"
    );
    let lifted = view.at;
    let mut frames = 0;
    while kinetic.running() && frames < 10_000 {
        kinetic.advance(&mut view, 16.7, false);
        frames += 1;
    }
    assert!(
        (view.at - lifted - scroll::tail_total(speed)).abs() < 1.,
        "the tail travelled its closed form"
    );
}

#[test]
fn a_stroke_past_the_end_resists_then_returns_on_outcubic_or_snaps_reduced() {
    for reduced in [false, true] {
        let mut kinetic = Kinetic::default();
        let mut view = axis(590., 600.);
        kinetic.wheel(&mut view, wheel(Phase::Begin, 0., 0.), 0., reduced);
        kinetic.wheel(&mut view, wheel(Phase::Update, -40., 0.), 8., reduced);
        assert!(
            view.at > 600. && view.at < 690.,
            "past the end the travel resists"
        );
        assert!(kinetic.overscrolled(&view));
        kinetic.wheel(&mut view, wheel(Phase::End, 0., 0.), 200., reduced);
        if reduced {
            assert_eq!(view.at, 600.);
            assert!(!kinetic.running());
            continue;
        }
        assert!(kinetic.return_active());
        let from = view.at;
        kinetic.advance(&mut view, 90., false);
        let expected = scroll::return_at(from, 600., 90., scroll::RETURN_MS);
        assert!(close(view.at, expected));
        kinetic.advance(&mut view, 90., false);
        assert!(close(view.at, 600.), "the return lands on its bound");
        assert!(!kinetic.running());
    }
}

#[test]
fn a_press_holds_motion_and_a_new_listing_halts_it_without_a_write() {
    let mut kinetic = Kinetic::default();
    let mut view = axis(1000., 90_000.);
    kinetic.wheel(&mut view, wheel(Phase::Begin, 0., 0.), 0., false);
    kinetic.wheel(&mut view, wheel(Phase::Update, -40., 0.), 8., false);
    kinetic.wheel(&mut view, wheel(Phase::End, 0., 0.), 16., false);
    assert!(kinetic.tail_active());
    let at = view.at;
    kinetic.press(&mut view);
    assert!(!kinetic.running());
    assert_eq!(view.at, at);
    kinetic.wheel(&mut view, wheel(Phase::Begin, 0., 0.), 20., false);
    kinetic.wheel(&mut view, wheel(Phase::Update, -40., 0.), 28., false);
    kinetic.wheel(&mut view, wheel(Phase::End, 0., 0.), 36., false);
    kinetic.halt();
    assert!(!kinetic.running() && kinetic.banked() == 0.);
}

#[test]
fn a_notch_glides_on_outcubic_over_the_reveal_duration_with_no_hold() {
    let to = 288.;
    let at = |elapsed: f32| {
        glided(
            Some(Glide {
                from: 0.,
                to,
                elapsed,
            }),
            to,
        )
    };
    assert_eq!(at(0.), 0.);
    assert!(
        at(16.) > 0.,
        "the first frame already moves, no artificial hold"
    );
    assert!((at(90.) - (to * scroll::out_cubic(0.5) as f32)).abs() < 1e-3);
    assert_eq!(at(scroll::RETURN_MS as f32), to);
    assert_eq!(glided(None, to), to);
}
