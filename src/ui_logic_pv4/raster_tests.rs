use super::camera::{self, View};
use super::color::{self, Ink, Palette};
use super::raster::{Prepared, Raster, Style, TOOLPATH_SEGMENTS};
use super::scene::{self, Layer, PlateRect, Scene, Segment};
use super::tests::stl_box;

pub(super) const DARK: Palette = Palette { ground: [0.07, 0.05, 0.05], foreground: [0.85, 0.78, 0.65], muted: [0.55, 0.5, 0.45], frame: [0.45, 0.4, 0.35] };
const LIGHT: Palette = Palette { ground: [0.96, 0.95, 0.93], foreground: [0.2, 0.2, 0.25], muted: [0.5, 0.5, 0.5], frame: [0.6, 0.6, 0.6] };

fn style(palette: Palette) -> Style<'static> {
    Style { palette, wire: None, layer: None, plate: None, ground: Some(palette.ground) }
}

fn draw(scene: &Scene, view: &View, style: &Style, w: usize, h: usize, ss: usize) -> Vec<u8> {
    let prepared = Prepared::new(scene, style.palette);
    let mut raster = Raster::default();
    raster.resize(w, h, ss);
    raster.render(scene, &prepared, 0, view, style);
    let mut out = Vec::new();
    raster.rgb_into(&mut out);
    out
}

fn px(rgb: &[u8], w: usize, x: usize, y: usize) -> [u8; 3] {
    let i = (y * w + x) * 3;
    [rgb[i], rgb[i + 1], rgb[i + 2]]
}

fn home(scene: &Scene, w: usize, h: usize) -> View {
    camera::home(scene, 0, w as f32, h as f32).unwrap().0
}

#[test]
fn a_box_is_flat_shaded_in_ten_percent_steps_of_the_foreground_on_the_ground() {
    let s = stl_box([20., 20., 0.], [80., 60., 40.]);
    let (w, h) = (200, 140);
    let rgb = draw(&s, &home(&s, w, h), &style(DARK), w, h, 1);
    let ground = color::to_bytes(DARK.ground);
    assert_eq!(px(&rgb, w, 0, 0), ground);
    let ink = Ink::from_lit(DARK.foreground);
    let steps: Vec<[u8; 3]> = (0..=10).map(|k| color::to_bytes(color::mix(ink.shade, ink.lit, k as f32 / 10.))).collect();
    let mut seen = std::collections::BTreeSet::new();
    for p in rgb.chunks(3) {
        let p = [p[0], p[1], p[2]];
        if p != ground {
            assert!(steps.contains(&p), "{p:?} is not a tone step");
            seen.insert(p);
        }
    }
    assert_eq!(seen.len(), 3, "an isometric box shows three faces in three tones");
}

#[test]
fn the_lit_face_is_the_lighter_one_in_a_light_theme_too() {
    let ink = Ink::from_lit(LIGHT.foreground);
    assert!(color::oklab(ink.lit)[0] > color::oklab(ink.shade)[0]);
}

#[test]
fn the_nearer_face_wins_whatever_order_the_file_lists_them() {
    let near = stl_box([0., 0., 20.], [10., 10., 30.]);
    let far = stl_box([0., 0., 0.], [10., 10., 10.]);
    let mut a = near.clone();
    let mut b = far.clone();
    for (into, from) in [(&mut a, &far), (&mut b, &near)] {
        let base = into.positions.len() as u32;
        into.positions.extend(&from.positions);
        into.triangles.extend(from.triangles.iter().map(|t| t.map(|i| i + base)));
    }
    let (w, h) = (120, 160);
    let view = home(&a, w, h);
    assert_eq!(draw(&a, &view, &style(DARK), w, h, 1), draw(&b, &view, &style(DARK), w, h, 1));
}

#[test]
fn a_face_turned_away_draws_nothing() {
    let mut s = Scene { positions: vec![[0., 0., 0.], [0., 10., 0.], [10., 0., 0.]], triangles: vec![[0, 1, 2]], ..Scene::default() };
    let (w, h) = (60, 60);
    let view = camera::frame(&s.positions, camera::ISO_AZ, camera::iso_el(), w as f32, h as f32, 0.9, None);
    let ground = color::to_bytes(DARK.ground);
    assert!(draw(&s, &view, &style(DARK), w, h, 1).chunks(3).all(|p| p == ground));
    s.triangles = vec![[0, 2, 1]];
    assert!(draw(&s, &view, &style(DARK), w, h, 1).chunks(3).any(|p| p != ground));
}

#[test]
fn the_plate_draws_muted_hairlines_and_its_edge_in_the_frame_role() {
    let s = stl_box([20., 20., 0.], [80., 60., 40.]);
    let (w, h) = (300, 200);
    let (view, rect, _) = camera::home(&s, 0, w as f32, h as f32).unwrap();
    assert_eq!(rect, PlateRect { x: 0., y: 0., w: 100., h: 80. });
    let mut st = style(DARK);
    let bare = draw(&s, &view, &st, w, h, 1);
    st.plate = Some(rect);
    let plated = draw(&s, &view, &st, w, h, 1);
    let changed: Vec<usize> = (0..w * h).filter(|&i| bare[i * 3..i * 3 + 3] != plated[i * 3..i * 3 + 3]).collect();
    assert!(changed.len() > 500, "{}", changed.len());
    let corner = view.project(&view.basis(), [100., 80., 0.]);
    let (cx, cy) = (corner[0].round() as usize, corner[1].round() as usize);
    let near_corner = (cy.saturating_sub(1)..=cy + 1).flat_map(|y| (cx.saturating_sub(1)..=cx + 1).map(move |x| (x, y))).any(|(x, y)| px(&plated, w, x, y) != px(&bare, w, x, y));
    assert!(near_corner, "the plate's corner at {cx},{cy} is drawn");
}

#[test]
fn wireframe_draws_seen_edges_bright_and_hidden_ones_faint_and_no_faces() {
    let s = stl_box([0.; 3], [40.; 3]);
    let edges = scene::edges(&s, 0);
    let (w, h) = (160, 160);
    let view = home(&s, w, h);
    let mut st = style(DARK);
    st.wire = Some(&edges);
    let rgb = draw(&s, &view, &st, w, h, 1);
    let ground = color::to_bytes(DARK.ground);
    let lit = |p: &[u8]| p.iter().zip(ground).map(|(&a, b)| (a as i32 - b as i32).abs()).max().unwrap_or(0);
    let strongest = rgb.chunks(3).map(lit).max().unwrap();
    let faces = draw(&s, &view, &style(DARK), w, h, 1);
    let inside: Vec<usize> = (0..w * h).filter(|&i| lit(&faces[i * 3..i * 3 + 3]) > 0).collect();
    let marked = inside.iter().filter(|&&i| lit(&rgb[i * 3..i * 3 + 3]) > 0).count();
    assert!(marked * 10 < inside.len() * 4, "the box's faces are not filled: {marked} of {}", inside.len());
    let full = rgb.chunks(3).filter(|p| lit(p) * 10 >= strongest * 8).count();
    let faint = rgb.chunks(3).filter(|p| lit(p) > 0 && lit(p) * 10 < strongest * 3).count();
    assert!(full > 60 && faint > 20, "{full} seen, {faint} hidden");
}

fn toolpath() -> Scene {
    let mut s = Scene::default();
    for (k, z) in [0.2f32, 0.4, 0.6].into_iter().enumerate() {
        let first = s.segments.len() as u32;
        s.segments.push(Segment { a: [0., 0., z], b: [0., 0., z], extrude: false });
        let x = k as f32 * 15.;
        s.segments.push(Segment { a: [x, 0., z], b: [x + 10., 0., z], extrude: true });
        s.segments.push(Segment { a: [x + 10., 0., z], b: [x + 10., 30., z], extrude: false });
        s.layers.push(Layer { segments: first..s.segments.len() as u32, z });
    }
    s.bed = Some([50., 40.]);
    s
}

#[test]
fn a_toolpath_hides_travel_and_the_layers_above_and_draws_the_current_one_in_the_role() {
    let s = toolpath();
    let (w, h) = (600, 400);
    let mut points = Vec::new();
    s.for_each_point(0, |p| points.push(p));
    let view = camera::frame(&points, camera::ISO_AZ, camera::iso_el(), w as f32, h as f32, 0.9, None);
    let mut st = style(DARK);
    let ground = color::to_bytes(DARK.ground);
    let drawn_near = |rgb: &[u8], p: [f32; 3]| {
        let q = view.project(&view.basis(), p);
        (q[1] as usize - 2..=q[1] as usize + 2).any(|y| (q[0] as usize - 2..=q[0] as usize + 2).any(|x| px(rgb, w, x, y) != ground))
    };
    st.layer = Some(1);
    let rgb = draw(&s, &view, &st, w, h, 1);
    assert!(drawn_near(&rgb, [5., 0., 0.2]) && drawn_near(&rgb, [20., 0., 0.4]));
    assert!(!drawn_near(&rgb, [35., 0., 0.6]), "the layer above is hidden");
    assert!(!drawn_near(&rgb, [25., 20., 0.4]), "travel is hidden");
    let lit = color::to_bytes(DARK.foreground);
    let q = view.project(&view.basis(), [20., 0., 0.4]);
    assert_eq!(px(&rgb, w, q[0] as usize, q[1] as usize), lit, "the current layer is the role itself");
    let q = view.project(&view.basis(), [5., 0., 0.2]);
    let below = px(&rgb, w, q[0] as usize, q[1] as usize);
    assert!(color::oklab(color::from_bytes(below))[0] < color::oklab(DARK.foreground)[0], "the layer below is dimmer");
}

#[test]
fn a_thumbnail_is_transparent_round_the_model_and_opaque_on_it() {
    let s = stl_box([0.; 3], [60., 40., 40.]);
    let rgba = super::render_thumbnail(&s, 0, 64, 64, DARK).unwrap();
    assert_eq!(rgba.len(), 64 * 64 * 4);
    assert_eq!(rgba[3], 0, "the corner is transparent");
    assert_eq!(rgba[(32 * 64 + 32) * 4 + 3], 255, "the middle is the model");
    assert!(super::render_thumbnail(&Scene::default(), 0, 64, 64, DARK).is_none());
}

#[test]
fn supersampling_softens_an_edge_and_the_same_frame_is_the_same_bytes() {
    let s = stl_box([0.; 3], [60., 40., 40.]);
    let (w, h) = (100, 80);
    let view = home(&s, w, h);
    let one = draw(&s, &view, &style(DARK), w, h, 1);
    let two = draw(&s, &view, &style(DARK), w, h, 2);
    assert_eq!(one, draw(&s, &view, &style(DARK), w, h, 1));
    let distinct = |rgb: &[u8]| rgb.chunks(3).collect::<std::collections::BTreeSet<_>>().len();
    assert!(distinct(&two) > distinct(&one) + 4, "{} vs {}", distinct(&two), distinct(&one));
}

#[test]
fn looking_up_from_under_the_plate_draws_the_plate_over_the_model() {
    let s = stl_box([20., 20., 0.], [80., 60., 40.]);
    let (w, h) = (200, 160);
    let (mut view, rect, _) = camera::home(&s, 0, w as f32, h as f32).unwrap();
    view.el = -0.5;
    let mut st = style(DARK);
    let bare = draw(&s, &view, &st, w, h, 1);
    st.plate = Some(rect);
    let plated = draw(&s, &view, &st, w, h, 1);
    let mid = view.project(&view.basis(), [50., 40., 0.]);
    let row = mid[1] as usize;
    let crossed = (0..w).filter(|&x| px(&bare, w, x, row) != color::to_bytes(DARK.ground) && px(&plated, w, x, row) != px(&bare, w, x, row)).count();
    assert!(crossed > 0, "a hairline crosses the model seen from under the plate");
}

#[test]
fn a_material_colour_draws_in_its_own_shades_beside_the_role() {
    let mut s = stl_box([0.; 3], [20.; 3]);
    s.colors = vec![Some([255, 106, 19]); 12];
    let prepared = Prepared::new(&s, DARK);
    assert_eq!(prepared.inks.len(), 2);
    assert_eq!(prepared.ink_of, vec![1u32; 12]);
    assert_eq!(prepared.inks[1].lit, color::from_bytes([255, 106, 19]));
    let bare = stl_box([0.; 3], [20.; 3]);
    let (dark, view) = (Prepared::new(&bare, DARK), home(&bare, 80, 80));
    let mut raster = Raster::default();
    raster.resize(80, 80, 1);
    raster.render(&bare, &dark, 0, &view, &style(LIGHT));
    let mut out = Vec::new();
    raster.rgb_into(&mut out);
    assert_eq!(out, draw(&bare, &view, &style(LIGHT), 80, 80, 1), "the role ink follows the frame's palette, not the one it was prepared with");
}

#[test]
fn a_long_print_draws_within_the_toolpath_budget() {
    let (layers, per_layer) = (400usize, 1_000usize);
    let mut s = Scene { bed: Some([250., 210.]), ..Scene::default() };
    for l in 0..layers {
        let z = 0.2 * (l + 1) as f32;
        let start = s.segments.len() as u32;
        for k in 0..per_layer {
            let a = k as f32 * 0.07;
            let r = 4. + 90. * k as f32 / per_layer as f32;
            s.segments.push(Segment { a: [125. + r * a.cos(), 105. + r * a.sin(), z], b: [125. + r * a.cos() + 0.3, 105. + r * a.sin(), z], extrude: true });
        }
        s.layers.push(Layer { segments: start..s.segments.len() as u32, z });
    }
    let (w, h) = (900, 545);
    let (view, rect, _) = camera::home(&s, 0, w as f32, h as f32).unwrap();
    let prepared = Prepared::new(&s, DARK);
    let mut raster = Raster::default();
    raster.resize(w, h, 1);
    let style = Style { palette: DARK, wire: None, layer: Some(layers - 1), plate: Some(rect), ground: Some(DARK.ground) };
    raster.render(&s, &prepared, 0, &view, &style);
    let drawn = raster.stats.segments;
    assert!(drawn > 0 && drawn <= TOOLPATH_SEGMENTS + layers, "drew {drawn} of {}", layers * per_layer);
}
