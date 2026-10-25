//! The oracle's pure decisions: Scroll.js's physics and SlowClick.js's replayed sequences, state after every step.
use super::{
    json::{map, V},
    vectors::{index, int, nums, qt_phase, real, Out},
    Lcg,
};
use flea_gui::input::{
    scroll::{self, Phase, Sample},
    slow::{SlowClick, SlowGate},
};

pub(super) fn scroll_functions(out: &mut Out, lcg: &mut Lcg) {
    let phases = [Phase::None, Phase::Begin, Phase::Update, Phase::End];
    for pixel in [0., -15., -10., 7.5, 120., -0.5] {
        for angle in [0., -120., 120., 240., -360., 15.] {
            for lines in [0., 1., 3., -1.] {
                for multiplier in [4., 1.] {
                    let args = nums(&[pixel, angle, lines, 24., multiplier]);
                    out.call(
                        "Scroll",
                        "distance",
                        args,
                        scroll::distance(pixel, angle, lines, 24., multiplier),
                    );
                }
            }
            for phase in phases {
                let args = vec![qt_phase(phase), pixel.into(), angle.into()];
                out.call(
                    "Scroll",
                    "movesByPixels",
                    args,
                    scroll::moves_by_pixels(phase, pixel, angle),
                );
            }
        }
        out.call(
            "Scroll",
            "touchDistance",
            nums(&[pixel]),
            scroll::touch_distance(pixel),
        );
    }
    for phase in phases {
        out.call(
            "Scroll",
            "isTouchpad",
            vec![qt_phase(phase)],
            scroll::is_touchpad(phase),
        );
    }
    for _ in 0..1500 {
        let mut t = real(lcg, 0., 50.);
        let mut samples = Vec::new();
        for _ in 0..lcg.below(9) {
            t += *lcg.pick(&[0., 1., 4., 8., 16., 30., 60., 120.]);
            let x = if lcg.below(3) == 0 {
                0.
            } else {
                real(lcg, -40., 40.)
            };
            let y = if lcg.below(4) == 0 {
                0.
            } else {
                real(lcg, -400., 400.)
            };
            samples.push(Sample { t, x, y });
        }
        let end = t + *lcg.pick(&[0., 5., 16., 50., 99., 100., 101., 150.]);
        let drawn: Vec<V> = samples
            .iter()
            .map(|s| {
                map(vec![
                    ("t", s.t.into()),
                    ("x", s.x.into()),
                    ("y", s.y.into()),
                ])
            })
            .collect();
        let (vx, vy) = scroll::lift_velocity(&samples, end);
        out.call(
            "Scroll",
            "liftVelocity",
            vec![V::List(drawn), end.into()],
            map(vec![("vx", vx.into()), ("vy", vy.into())]),
        );
    }
    let speeds = [-6., -1.5, -0.03, 0., 0.02, 0.03, 0.031, 0.5, 2.25, 6.];
    for v in speeds {
        out.call("Scroll", "tailTotal", nums(&[v]), scroll::tail_total(v));
        for ms in [0., 1., 8., 16.5, 100., 500., 3000., -4.] {
            out.call(
                "Scroll",
                "tailTravel",
                nums(&[v, ms]),
                scroll::tail_travel(v, ms),
            );
            let (dx, next) = scroll::tail_step(v, ms);
            out.call(
                "Scroll",
                "tailStep",
                nums(&[v, ms]),
                map(vec![("dx", dx.into()), ("v", next.into())]),
            );
            out.call(
                "Scroll",
                "overDecel",
                nums(&[v, ms]),
                scroll::over_decel(v, ms),
            );
        }
        for w in speeds {
            out.call(
                "Scroll",
                "tailSpeed",
                nums(&[v, w]),
                scroll::tail_speed(v, w),
            );
            out.call("Scroll", "tailLive", nums(&[v, w]), scroll::tail_live(v, w));
        }
    }
    for _ in 0..1500 {
        let (origin, value) = (real(lcg, -20., 20.), real(lcg, -100., 2500.));
        let (raw, length) = (real(lcg, -10., 600.), real(lcg, 0., 800.));
        out.call(
            "Scroll",
            "overResist",
            nums(&[raw, length]),
            scroll::over_resist(raw, length),
        );
        let (elapsed, duration) = (real(lcg, -10., 250.), *lcg.pick(&[0., 1., 180., 0.5, 300.]));
        out.call(
            "Scroll",
            "returnAt",
            nums(&[value, origin, elapsed, duration]),
            scroll::return_at(value, origin, elapsed, duration),
        );
        let current = value + *lcg.pick(&[0., 0.005, 0.01, 0.0101, 0.5, -0.02, -0.01]);
        out.call(
            "Scroll",
            "moved",
            nums(&[value, current]),
            scroll::moved(value, current),
        );
    }
}

fn gate(lcg: &mut Lcg) -> SlowGate {
    SlowGate {
        single_click: lcg.below(8) == 0,
        click_rename: lcg.below(8) != 0,
        search: lcg.below(8) == 0,
        renaming: lcg.below(8) == 0,
        band: lcg.below(8) == 0,
        drag: lcg.below(8) == 0,
        menu: lcg.below(8) == 0,
    }
}

fn gate_value(gate: SlowGate) -> V {
    map(vec![
        ("single", gate.single_click.into()),
        ("clickRename", gate.click_rename.into()),
        ("search", gate.search.into()),
        ("renaming", gate.renaming.into()),
        ("band", gate.band.into()),
        ("drag", gate.drag.into()),
        ("menu", gate.menu.into()),
    ])
}

pub(super) fn slow_vectors(out: &mut Out, lcg: &mut Lcg) {
    for _ in 0..1000 {
        let mut slow = SlowClick::default();
        let (mut steps, mut answers) = (Vec::new(), Vec::new());
        let mut now = 0u64;
        for _ in 0..1 + lcg.below(10) {
            let gate = gate(lcg);
            let answer = match lcg.below(5) {
                0..=2 => {
                    now += *lcg.pick(&[0, 100, 399, 400, 401, 800]);
                    let (index, modifiers, sole) = (
                        int(lcg, -1, 2),
                        *lcg.pick(&[0u8, 0, 1, 2]),
                        lcg.below(4) != 0,
                    );
                    let interval = *lcg.pick(&[400u64, 250]);
                    let armed = slow.arm(index, modifiers, now, interval, gate, sole);
                    steps.push(map(vec![
                        ("op", "arm".into()),
                        ("index", index.into()),
                        ("mods", u64::from(modifiers).into()),
                        ("now", now.into()),
                        ("interval", interval.into()),
                        ("sole", sole.into()),
                        ("gate", gate_value(gate)),
                    ]));
                    V::from(armed)
                }
                3 => {
                    let selected: Vec<usize> = (0..3).filter(|_| lcg.below(2) == 0).collect();
                    let cursor = lcg.below(3) as usize;
                    let fired = slow.fire(gate, &selected, cursor);
                    steps.push(map(vec![
                        ("op", "fire".into()),
                        ("selected", selected.into()),
                        ("cursor", cursor.into()),
                        ("gate", gate_value(gate)),
                    ]));
                    index(fired)
                }
                _ => {
                    slow.cancel();
                    steps.push(map(vec![("op", "cancel".into())]));
                    V::Null
                }
            };
            answers.push(V::List(vec![answer, slow.index.into()]));
        }
        out.row(
            "slow",
            vec![("steps", V::List(steps)), ("answers", V::List(answers))],
        );
    }
}
