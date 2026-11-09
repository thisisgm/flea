//! Untrusted sizes, coordinates, colours and indices: every frame and thumbnail finishes, and a bad scene is refused.
use super::camera;
use super::color::from_bytes;
use super::raster::{Prepared, Raster, Style};
use super::raster_tests::DARK;
use super::render_thumbnail;
use super::scene::{bed_from_metadata, decimate, Layer, Scene, Segment};
use super::tests::stl_box;
use std::sync::mpsc;
use std::time::Duration;

// Runs `work` on its own thread and answers its result if it finished within `seconds`.
fn within<T: Send + 'static>(seconds: u64, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(Duration::from_secs(seconds)).ok()
}

// Draws the scene's home view with its plate at 300 by 200, two samples a side, and answers whether it finished.
fn plated_frame(scene: Scene) -> bool {
    within(20, move || {
        let (w, h) = (300, 200);
        let Some((view, rect, _)) = camera::home(&scene, 0, w as f32, h as f32) else { return };
        let prepared = Prepared::new(&scene, DARK);
        let mut raster = Raster::default();
        raster.resize(w, h, 2);
        let style = Style { palette: DARK, wire: None, layer: None, plate: Some(rect), ground: Some(DARK.ground) };
        raster.render(&scene, &prepared, 0, &view, &style);
    })
    .is_some()
}

#[test]
fn an_absurd_printer_plate_is_ignored_and_any_plate_draws_in_bounded_time() {
    assert_eq!(bed_from_metadata([("printable_area", "0x0,1e30x0,1e30x1e30,0x1e30")]), None);
    assert_eq!(bed_from_metadata([("machine_width", "1e7"), ("machine_depth", "1e7")]), None);
    assert_eq!(bed_from_metadata([("machine_width", "10000"), ("machine_depth", "250")]), Some([10000., 250.]));
    for side in [1e7f32, 1e30, f32::INFINITY] {
        let s = Scene { bed: Some([side, side]), ..stl_box([20., 20., 0.], [80., 60., 40.]) };
        assert!(plated_frame(s), "bed {side}");
    }
    for corner in [1e7f32, 1e30, f32::INFINITY, f32::NAN] {
        let mut s = stl_box([20., 20., 0.], [80., 60., 40.]);
        s.positions[0][0] = corner;
        assert!(plated_frame(s), "corner {corner}");
    }
}

#[test]
fn a_hundred_thousand_face_colours_prepare_in_bounded_time_and_each_face_keeps_its_colour() {
    let n = 100_000u32;
    let mut s = Scene::default();
    for k in 0..n {
        let x = (k % 400) as f32;
        let y = (k / 400) as f32;
        s.positions.extend([[x, y, 0.], [x + 1., y, 0.], [x, y + 1., 0.]]);
        s.triangles.push([3 * k, 3 * k + 1, 3 * k + 2]);
        s.colors.push(Some([k as u8, (k >> 8) as u8, (k >> 16) as u8]));
    }
    let colors = s.colors.clone();
    let prepared = within(20, move || Prepared::new(&s, DARK)).expect("prepared within 20 s");
    assert!(prepared.inks.len() <= 1 + 2 * 4096, "{} inks", prepared.inks.len());
    for (t, c) in colors.iter().enumerate() {
        let lit = prepared.inks[prepared.ink_of[t] as usize].lit;
        let want = from_bytes(c.unwrap());
        assert!((0..3).all(|i| (lit[i] - want[i]).abs() <= 8.5 / 255.), "face {t}: {lit:?} for {want:?}");
    }
    let exact = prepared.inks[prepared.ink_of[100] as usize].lit;
    assert_eq!(exact, from_bytes(colors[100].unwrap()), "the first colours stay exact");
}

#[test]
fn a_scene_with_an_index_past_its_positions_is_refused_not_drawn() {
    let s = Scene { positions: vec![[0.; 3], [1., 0., 0.], [0., 1., 0.]], triangles: vec![[0, 1, 9]], ..Scene::default() };
    let thumb = std::panic::catch_unwind(|| render_thumbnail(&s, 0, 64, 64, DARK));
    assert!(matches!(thumb, Ok(None)), "refused without a panic");
    let short = Scene { colors: vec![Some([1, 2, 3])], ..stl_box([0.; 3], [10.; 3]) };
    assert!(matches!(std::panic::catch_unwind(|| render_thumbnail(&short, 0, 64, 64, DARK)), Ok(None)), "colours short of the triangles");
}

#[test]
fn a_live_plate_keeps_only_the_positions_it_uses_welded_and_a_far_plate_thins_its_hairlines() {
    let s = stl_box([0.; 3], [10.; 3]);
    assert_eq!(s.positions.len(), 36);
    let live = decimate(&s, 0, 1_000);
    assert_eq!((live.positions.len(), live.triangles.len()), (8, 12));
    let mut two = stl_box([0.; 3], [10.; 3]);
    two.triangles.extend(stl_box([50.; 3], [60.; 3]).triangles.iter().map(|t| t.map(|i| i + 36)));
    two.positions.extend(stl_box([50.; 3], [60.; 3]).positions);
    two.plates = vec![0..12, 12..24];
    let second = decimate(&two, 1, 1_000);
    assert!(second.positions.len() == 8 && second.positions.iter().all(|p| p[0] >= 50.), "only the second plate's corners");
    let wide = Scene { bed: Some([10_000., 10_000.]), ..stl_box([20., 20., 0.], [80., 60., 40.]) };
    assert_eq!(wide.check(), Ok(()));
    let started = std::time::Instant::now();
    assert!(plated_frame(wide));
    assert!(started.elapsed() < Duration::from_secs(5), "a 10 m plate draws its hairlines no closer than 2 px");
}

#[test]
fn the_bounds_follow_the_parser_not_a_10_m_box() {
    assert_eq!(stl_box([15_000., 20., 0.], [15_100., 100., 40.]).check(), Ok(()), "a 100 mm part at x = 15 m");
    assert_eq!(stl_box([1e9 - 128., 1e9 - 64., 0.], [1e9, 1e9, 40.]).check(), Ok(()), "a part at the parser's 1e9 mm");
    assert!(stl_box([-1.5e9, 0., 0.], [1.5e9, 10., 10.]).check().is_err(), "a point past the parser's 1e9 mm");
}

#[test]
fn layers_out_of_order_are_refused() {
    let mv = Segment { a: [0.; 3], b: [1., 0., 0.], extrude: true };
    let s = Scene { segments: vec![mv; 4], layers: vec![Layer { segments: 2..4, z: 0.2 }, Layer { segments: 0..2, z: 0.4 }], ..Scene::default() };
    assert_eq!(s.check(), Err("layers out of order"));
}

#[test]
fn keep_extrusions_drops_travel_and_renumbers_each_layer() {
    let mv = |extrude| Segment { a: [0.; 3], b: [1., 0., 0.], extrude };
    let mut s = Scene {
        segments: vec![mv(false), mv(true), mv(true), mv(false), mv(true), mv(false), mv(false), mv(true)],
        layers: vec![Layer { segments: 0..4, z: 0.2 }, Layer { segments: 4..8, z: 0.4 }],
        ..Scene::default()
    };
    assert_eq!(s.check(), Ok(()));
    s.keep_extrusions();
    assert_eq!(s.segments.len(), 4);
    assert!(s.segments.iter().all(|m| m.extrude));
    assert_eq!(s.layers.iter().map(|l| l.segments.clone()).collect::<Vec<_>>(), vec![0..2, 2..4]);
}
