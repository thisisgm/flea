use super::camera::{self, Framing};
use super::control::{action, Action, Surface, Viewer};
use super::motion::{out_cubic, EASE_IN, INERTIA, TURN_SECONDS};
use super::scene::{Layer, Scene, Segment};
use super::tests::stl_box;
use std::f32::consts::PI;

const W: f32 = 900.;
const H: f32 = 546.;
const FRAME: f64 = 1. / 60.;

fn bracket() -> Scene {
    stl_box([20., 20., 0.], [80., 60., 40.])
}

fn look() -> Viewer {
    Viewer::new(Surface::QuickLook, &bracket(), 0, W, H).unwrap()
}

// Advances frame by frame from `from` to `to`, as a presented display would.
fn run(v: &mut Viewer, from: f64, to: f64) {
    let mut t = from;
    while t < to {
        t = (t + FRAME).min(to);
        v.advance(t);
    }
}

#[test]
fn the_turntable_eases_in_over_0_6_s_then_turns_once_in_24_s() {
    let mut v = look();
    v.opened(0., true);
    assert!(v.spinning());
    let start = v.view.az;
    run(&mut v, 0., 0.3);
    let rate = 2. * PI / TURN_SECONDS;
    let eased: f32 = (1..=18).map(|k| rate * out_cubic(k as f32 / 60. / EASE_IN) / 60.).sum();
    assert!((v.view.az - start - eased).abs() < 1e-4, "{} vs {}", v.view.az - start, eased);
    run(&mut v, 0.3, 1.);
    let at = v.view.az;
    run(&mut v, 1., 25.);
    assert!((v.view.az - at - 2. * PI).abs() < 0.01, "one turn in 24 s: {}", v.view.az - at);
}

#[test]
fn the_first_drag_wheel_or_orbit_key_stops_the_turn_where_it_is() {
    for stop in 0..3 {
        let mut v = look();
        v.opened(0., true);
        run(&mut v, 0., 2.);
        match stop {
            0 => v.press(2.),
            1 => v.wheel(450., 273., -120., false, 2.),
            _ => assert!(v.act(Action::OrbitLeft, 2., true)),
        }
        assert!(!v.spinning(), "{stop}");
    }
}

#[test]
fn p_toggles_the_turn_and_only_quick_look_or_live_animations_ever_start_it() {
    let mut v = look();
    v.opened(0., true);
    assert!(v.act(Action::ToggleTurn, 1., true));
    assert!(!v.spinning() && v.turn_off);
    v.opened(2., true);
    assert!(!v.spinning(), "paused by p stays paused when shown again");
    assert!(v.act(Action::ToggleTurn, 3., true));
    assert!(v.spinning() && !v.turn_off);
    let mut off = look();
    off.opened(0., false);
    assert!(!off.spinning(), "never starts while Hyprland's animations are off");
    assert!(!off.act(Action::ToggleTurn, 1., false) && !off.spinning());
    let mut column = Viewer::new(Surface::Column, &bracket(), 0, W, H).unwrap();
    column.opened(0., true);
    assert!(!column.spinning() && !column.act(Action::ToggleTurn, 1., true));
    let mut terminal = Viewer::new(Surface::Terminal, &bracket(), 0, W, H).unwrap();
    terminal.opened(0., true);
    assert!(!terminal.spinning());
}

#[test]
fn a_paused_view_does_not_move_and_resumes_from_rest() {
    let mut v = look();
    v.opened(0., true);
    run(&mut v, 0., 3.);
    v.pause();
    let held = v.view.az;
    v.advance(10.);
    assert_eq!(v.view.az, held, "an unfocused or hidden view takes no step");
    v.resume(10.);
    run(&mut v, 10., 10.1);
    let rate = 2. * PI / TURN_SECONDS;
    assert!(v.view.az - held < rate * 0.1 * 0.7, "the turn eases in again after a pause");
}

#[test]
fn a_released_drag_glides_on_for_0_8_s_and_stops() {
    let mut v = look();
    v.press(0.);
    for k in 1..=6 {
        v.drag(-10., 0., false, k as f64 * 0.016);
    }
    let held = v.view.az;
    v.release(false, 0.1);
    assert!(v.moving());
    run(&mut v, 0.1, 0.1 + INERTIA as f64 + 0.05);
    assert!(!v.moving());
    let speed = 10. * camera::ORBIT_PER_PX / 0.016;
    let glide = v.view.az - held;
    assert!(glide > 0. && (glide - speed * INERTIA / 3.).abs() < speed * 0.05, "{glide} vs {}", speed * INERTIA / 3.);
}

#[test]
fn a_drag_that_stopped_before_release_does_not_glide_and_a_pan_never_does() {
    let mut v = look();
    v.press(0.);
    v.drag(-10., 0., false, 0.016);
    v.release(false, 0.2);
    assert!(!v.moving());
    v.press(1.);
    v.drag(30., 10., true, 1.016);
    v.release(true, 1.02);
    assert!(!v.moving());
    assert_ne!(v.view.target, v.home.target, "a pan moves the turn axis");
}

#[test]
fn the_pitch_stops_at_89_degrees_either_way_and_the_turn_has_no_limit() {
    let mut v = look();
    v.press(0.);
    v.drag(0., 100_000., false, 0.01);
    assert!((v.view.el - 89f32.to_radians()).abs() < 1e-6);
    v.drag(0., -200_000., false, 0.02);
    assert!((v.view.el + 89f32.to_radians()).abs() < 1e-6);
    v.drag(-1_000_000., 0., false, 0.03);
    assert!(v.view.az > 100.);
}

#[test]
fn orbit_keys_turn_15_degrees_and_zoom_keys_step_by_a_quarter() {
    let mut v = look();
    let az = v.view.az;
    v.act(Action::OrbitLeft, 0., true);
    run(&mut v, 0., 0.3);
    assert!((v.view.az - az - 15f32.to_radians()).abs() < 1e-5);
    v.act(Action::OrbitRight, 1., true);
    run(&mut v, 1., 1.3);
    assert!((v.view.az - az).abs() < 1e-5);
    let s = v.view.scale;
    v.act(Action::ZoomIn, 2., true);
    run(&mut v, 2., 2.25);
    assert!((v.view.scale / s - 1.25).abs() < 1e-4);
    v.act(Action::ZoomOut, 3., true);
    run(&mut v, 3., 3.25);
    assert!((v.view.scale / s - 1.).abs() < 1e-4);
}

#[test]
fn the_wheel_zooms_toward_the_pointer_within_set_limits() {
    let mut v = look();
    let grip = [80., 60., 40.];
    let b = v.view.basis();
    let before = v.view.project(&b, grip);
    v.wheel(before[0], before[1], -240., false, 0.);
    let after = v.view.project(&v.view.basis(), grip);
    assert!(v.view.scale > v.home.scale);
    assert!((after[0] - before[0]).abs() < 1e-2 && (after[1] - before[1]).abs() < 1e-2, "{before:?} {after:?}");
    for _ in 0..200 {
        v.wheel(450., 273., -500., false, 0.);
    }
    let near = v.view.scale;
    for _ in 0..400 {
        v.wheel(450., 273., 500., false, 0.);
    }
    let far = v.view.scale;
    let (lo, hi) = bracket().bounds(0).unwrap();
    let limits = camera::zoom_limits(camera::radius(lo, hi), H);
    assert!((near - limits.1).abs() < 1e-3 && (far - limits.0).abs() < 1e-3, "{near} {far} {limits:?}");
}

#[test]
fn reset_and_double_click_frame_the_model_again_the_short_way_round() {
    let mut v = look();
    v.press(0.);
    v.drag(-400., 80., false, 0.01);
    v.drag(25., -10., true, 0.02);
    v.wheel(10., 10., -300., false, 0.03);
    v.act(Action::Reset, 1., true);
    run(&mut v, 1., 1.35);
    let turned = (v.view.az - v.home.az).rem_euclid(2. * PI);
    assert!(turned < 1e-4 || 2. * PI - turned < 1e-4, "{turned}");
    assert_eq!((v.view.el, v.view.scale, v.view.cx, v.view.cy, v.view.target), (v.home.el, v.home.scale, v.home.cx, v.home.cy, v.home.target));
    v.view.az += 2. * PI * 3. + 0.2;
    let before = v.view.az;
    v.double_click(2.);
    run(&mut v, 2., 2.35);
    assert!((before - v.view.az - 0.2).abs() < 1e-4, "turns the short way: {}", before - v.view.az);
}

#[test]
fn the_start_frames_the_plates_sweep_so_a_turn_never_clips() {
    let v = look();
    assert_eq!(v.framing, Framing::Model);
    let mut view = v.home;
    for step in 0..24 {
        view.az = v.home.az + step as f32 * PI / 12.;
        let b = view.basis();
        for p in v.rect.corners() {
            let q = view.project(&b, p);
            assert!(q[0] >= 0. && q[0] <= W && q[1] >= 0. && q[1] <= H, "corner {p:?} at turn {step}: {q:?}");
        }
    }
}

#[test]
fn a_printers_plate_is_framed_whole_and_inset() {
    let mut s = bracket();
    s.bed = Some([256., 256.]);
    let v = Viewer::new(Surface::QuickLook, &s, 0, W, H).unwrap();
    assert_eq!(v.framing, Framing::Plate);
    let b = v.view.basis();
    let qs: Vec<[f32; 3]> = v.rect.corners().iter().map(|&p| v.view.project(&b, p)).collect();
    let (x0, x1) = qs.iter().fold((f32::MAX, f32::MIN), |(a, c), q| (a.min(q[0]), c.max(q[0])));
    let (y0, y1) = qs.iter().fold((f32::MAX, f32::MIN), |(a, c), q| (a.min(q[1]), c.max(q[1])));
    assert!(x0 > 0. && x1 < W && y0 > 0. && y1 < H);
    assert!(((x1 - x0) / W).max((y1 - y0) / H) > 0.8, "the plate fills its share of the view");
}

fn kit() -> Scene {
    let mut s = bracket();
    let base = s.triangles.len() as u32;
    let knob = stl_box([150., 150., 0.], [180., 180., 16.]);
    let at = s.positions.len() as u32;
    s.positions.extend(knob.positions);
    s.triangles.extend(knob.triangles.iter().map(|t| t.map(|i| i + at)));
    s.plates = vec![0..base, base..s.triangles.len() as u32];
    s
}

#[test]
fn plates_step_with_wrapping_and_a_single_plate_ignores_the_keys() {
    let s = kit();
    let v = Viewer::new(Surface::QuickLook, &s, 0, W, H).unwrap();
    assert_eq!(v.plates(), 2);
    let mut v2 = v.clone();
    assert!(v2.act(Action::PlateNext, 0., true));
    assert_eq!((v.stepped_plate(Action::PlateNext), v.stepped_plate(Action::PlatePrevious)), (1, 1));
    let second = Viewer::new(Surface::QuickLook, &s, 1, W, H).unwrap();
    assert_eq!((second.plate, second.stepped_plate(Action::PlateNext)), (1, 0));
    assert_eq!(Viewer::new(Surface::QuickLook, &s, 9, W, H).unwrap().plate, 1, "an out of range plate clamps");
    let mut single = look();
    assert!(!single.act(Action::PlateNext, 0., true) && !single.act(Action::PlatePrevious, 0., true));
}

fn print() -> Scene {
    let mut s = Scene::default();
    for k in 0..240 {
        let z = 0.2 * (k + 1) as f32;
        let first = s.segments.len() as u32;
        s.segments.push(Segment { a: [100., 100., z], b: [120., 100., z], extrude: true });
        s.layers.push(Layer { segments: first..first + 1, z });
    }
    s.bed = Some([250., 210.]);
    s
}

#[test]
fn layers_step_within_their_bounds_and_the_row_seeks_them() {
    let mut v = Viewer::new(Surface::QuickLook, &print(), 0, W, H).unwrap();
    assert_eq!((v.layer, v.layers()), (Some(239), 240), "a print opens whole, at its top layer");
    assert!(!v.act(Action::LayerUp, 0., true), "the top layer is the last");
    assert!(v.act(Action::LayerDown, 0., true));
    assert_eq!(v.layer, Some(238));
    assert!(v.seek_layer(128. / 240.));
    assert_eq!(v.layer, Some(127));
    assert!((v.layer_share().unwrap() - 128. / 240.).abs() < 1e-6);
    v.seek_layer(-5.);
    assert_eq!(v.layer, Some(0));
    assert!(!v.act(Action::LayerDown, 0., true), "the first layer is the floor");
    assert!(!v.act(Action::Wireframe, 0., true), "a toolpath has no wireframe");
    let mut mesh = look();
    assert!(!mesh.act(Action::LayerUp, 0., true) && mesh.layer.is_none());
    assert!(mesh.act(Action::Wireframe, 0., true) && mesh.wire);
}

#[test]
fn a_resize_reframes_home_and_a_view_at_home_follows() {
    let s = bracket();
    let mut v = look();
    v.resize(600., 400.);
    assert_eq!(v.view, v.home);
    assert_eq!(v.home, camera::home(&s, 0, 600., 400.).unwrap().0);
    v.view.az += 1.;
    v.resize(700., 400.);
    assert_ne!(v.view, v.home);
}

#[test]
fn the_preview_contexts_action_names_map_to_moves() {
    for (name, want) in [
        ("seekBack", Action::OrbitLeft), ("seekForward", Action::OrbitRight), ("zoomIn", Action::ZoomIn), ("zoomOut", Action::ZoomOut),
        ("zoomFit", Action::Reset), ("playPause", Action::ToggleTurn), ("toggleWrap", Action::Wireframe),
        ("tabPrevious", Action::PlatePrevious), ("tabNext", Action::PlateNext), ("pageUp", Action::LayerUp), ("pageDown", Action::LayerDown),
    ] {
        assert_eq!(action(name), Some(want), "{name}");
    }
    for name in ["cursorDown", "escape", "preview", ""] {
        assert_eq!(action(name), None, "{name}");
    }
}

#[test]
fn an_empty_scene_has_no_view() {
    assert!(Viewer::new(Surface::QuickLook, &Scene::default(), 0, W, H).is_none());
}
