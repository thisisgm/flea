use super::color::{self, Ink};
use super::facts::{self, FileFacts};
use super::scene::{self, bed_from_metadata, decimate, plate_rect, PlateRect, Scene, Segment};
use super::wire::{self, State};
use crate::i18n::{I18n, Paths};
use std::path::PathBuf;

// English from the source catalog alone: no Omarchy roster and no language file, so nothing is written.
pub(super) fn english() -> I18n {
    let none = PathBuf::from("/nonexistent/flea-pv4");
    I18n::load(Paths { omarchy: none.clone(), language_file: none, catalogs: vec![crate::i18n::test_catalogs::english_only()] })
}

/// An axis-aligned box as an STL carries it: twelve triangles, each with its own three corners.
pub(super) fn stl_box(lo: [f32; 3], hi: [f32; 3]) -> Scene {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [1, 3, 7, 5], [3, 2, 6, 7], [2, 0, 4, 6]];
    let mut s = Scene::default();
    for q in quads {
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            let base = s.positions.len() as u32;
            s.positions.extend(t.map(c));
            s.triangles.push([base, base + 1, base + 2]);
        }
    }
    s
}

#[test]
fn a_box_has_outward_normals_and_its_bounds() {
    let s = stl_box([20., 20., 0.], [80., 60., 40.]);
    assert_eq!(s.bounds(0), Some(([20., 20., 0.], [80., 60., 40.])));
    let top = (0..12).map(|t| s.normal(t)).filter(|n| n[2] > 0.99).count();
    let bottom = (0..12).map(|t| s.normal(t)).filter(|n| n[2] < -0.99).count();
    assert_eq!((top, bottom), (2, 2));
}

#[test]
fn bed_size_comes_from_each_slicers_metadata_and_refuses_malformed_text() {
    assert_eq!(bed_from_metadata([("bed_shape", "0x0,250x0,250x210,0x210")]), Some([250., 210.]));
    assert_eq!(bed_from_metadata([("printable_area", "0x0,256x0,256x256,0x256")]), Some([256., 256.]));
    assert_eq!(bed_from_metadata([("machine_width", "220"), ("machine_depth", "220")]), Some([220., 220.]));
    for bad in ["", "abc", "0x0,1x1", "0x0,NaNx0,1x1", "0x0,x,1x1", "0x0,0x0,0x0"] {
        assert_eq!(bed_from_metadata([("bed_shape", bad)]), None, "{bad}");
    }
    assert_eq!(bed_from_metadata([("machine_width", "220")]), None);
    assert_eq!(bed_from_metadata([("machine_width", "-1"), ("machine_depth", "220")]), None);
    assert_eq!(bed_from_metadata([("printer_model", "MK4")]), None);
}

#[test]
fn an_stl_plate_is_its_footprint_and_two_cells_and_a_printer_plate_is_whole() {
    assert_eq!(plate_rect([20., 20., 0.], [80., 60., 40.], None, [0., 0.]), PlateRect { x: 0., y: 0., w: 100., h: 80. });
    assert_eq!(plate_rect([70., 80., 0.], [180., 176., 40.], Some([256., 256.]), [0., 0.]), PlateRect { x: 0., y: 0., w: 256., h: 256. });
    // A second Bambu plate laid beside the first centres the printer's rectangle on its objects.
    let later = plate_rect([300., 100., 0.], [340., 140., 16.], Some([256., 256.]), [0., 0.]);
    assert_eq!(later, PlateRect { x: 192., y: -8., w: 256., h: 256. });
    // A part recentred by an offset of (100, 100) keeps the printer's plate under it.
    assert_eq!(plate_rect([-30., -30., 0.], [30., 30., 40.], Some([256., 256.]), [100., 100.]), PlateRect { x: -100., y: -100., w: 256., h: 256. });
}

#[test]
fn a_toolpath_frames_its_extrusion_and_a_travel_only_file_frames_every_move() {
    let mut s = Scene {
        segments: vec![
            Segment { a: [0., 0., 0.2], b: [100., 100., 0.2], extrude: false },
            Segment { a: [10., 10., 0.2], b: [20., 10., 0.2], extrude: true },
        ],
        ..Scene::default()
    };
    assert!(s.is_toolpath());
    assert_eq!(s.bounds(0), Some(([10., 10., 0.2], [20., 10., 0.2])));
    s.segments[1].extrude = false;
    assert_eq!(s.bounds(0), Some(([0., 0., 0.2], [100., 100., 0.2])));
    assert_eq!(Scene::default().bounds(0), None);
}

#[test]
fn plates_are_triangle_ranges_and_a_missing_plate_is_empty() {
    let mut s = stl_box([0.; 3], [1.; 3]);
    s.plates = vec![0..8, 8..12];
    assert_eq!((s.plate_triangles(0), s.plate_triangles(1), s.plate_triangles(2)), (0..8, 8..12, 0..0));
    assert_eq!(s.plate_count(), 2);
    assert!(s.bounds(2).is_none());
}

#[test]
fn an_unwelded_box_has_twelve_feature_edges_and_six_flat_diagonals() {
    let s = stl_box([0.; 3], [10.; 3]);
    let edges = scene::edges(&s, 0);
    assert_eq!(edges.len(), 18);
    assert_eq!(edges.iter().filter(|e| e.feature).count(), 12);
    assert!(edges.iter().all(|e| e.f2 != u32::MAX), "every welded edge joins two faces");
}

#[test]
fn faces_meeting_under_25_degrees_make_no_feature_edge_and_over_it_do() {
    for (degrees, feature) in [(20f32, false), (30., true)] {
        let a = degrees.to_radians();
        let s = Scene {
            positions: vec![[0., 0., 0.], [0., 1., 0.], [-1., 0.5, 0.], [a.cos(), 0.5, a.sin()]],
            triangles: vec![[0, 1, 2], [1, 0, 3]],
            ..Scene::default()
        };
        let shared = scene::edges(&s, 0).into_iter().find(|e| e.f2 != u32::MAX).unwrap();
        assert_eq!(shared.feature, feature, "{degrees}");
    }
}

#[test]
fn an_open_edge_is_always_a_feature() {
    let s = Scene { positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]], triangles: vec![[0, 1, 2]], ..Scene::default() };
    assert!(scene::edges(&s, 0).iter().all(|e| e.feature && e.f2 == u32::MAX));
}

fn grid_mesh(n: usize) -> Scene {
    let mut s = Scene::default();
    for y in 0..=n {
        for x in 0..=n {
            s.positions.push([x as f32, y as f32, ((x * 7 + y * 3) % 5) as f32 * 0.1]);
        }
    }
    for y in 0..n {
        for x in 0..n {
            let i = (y * (n + 1) + x) as u32;
            let j = i + n as u32 + 1;
            s.triangles.push([i, i + 1, j]);
            s.triangles.push([i + 1, j + 1, j]);
            s.colors.push(Some([200, 100, 0]));
            s.colors.push(None);
        }
    }
    s
}

#[test]
fn decimation_keeps_under_the_target_and_leaves_the_source_whole() {
    let s = grid_mesh(200);
    let before = s.clone();
    let live = decimate(&s, 0, 10_000);
    assert!(live.triangles.len() <= 10_000 && live.triangles.len() > 1_000, "{}", live.triangles.len());
    assert_eq!(live.colors.len(), live.triangles.len());
    assert!(live.triangles.iter().all(|t| t.iter().all(|&i| (i as usize) < live.positions.len())));
    assert_eq!(s, before);
    let small = decimate(&s, 0, 1_000_000);
    assert_eq!(small.triangles.len(), s.triangles.len());
    assert_eq!(scene::LIVE_TRIANGLES, 2_000_000);
}

#[test]
fn a_toolpath_is_never_decimated() {
    let s = Scene { segments: vec![Segment { a: [0.; 3], b: [1., 0., 0.], extrude: true }; 3], ..Scene::default() };
    assert_eq!(decimate(&s, 0, 1), s);
}

#[test]
fn request_and_cancel_lines_are_exact_and_escape_the_path() {
    assert_eq!(wire::request("look", 7, "/p/kit.3mf", Some(0)), r#"{"c":"model3d","slot":"look","token":7,"path":"/p/kit.3mf","plate":0}"#);
    assert_eq!(wire::request("column", 8, "/p/a\"b.stl", None), r#"{"c":"model3d","slot":"column","token":8,"path":"/p/a\"b.stl"}"#);
    assert_eq!(wire::cancel("look", 9), r#"{"c":"model3d","slot":"look","token":9,"cancel":true}"#);
}

pub(super) const READY: &str = r#"{"t":"model3d","slot":"look","token":7,"state":"ready","file":"/c/KEY.3d","thumbnail":"/c/KEY-all.png","data":{"format":"3mf","facts":{"dimensions_mm":[110,96,40],"volume_cm3":null,"surface_cm2":null,"triangles":5000,"objects":4,"plates":2,"open_edges":0},"slicer":{"printer":"Bambu Lab P1S","filament":"Bambu PLA Basic","nozzle_mm":[0.4],"layer_height_mm":0.2,"print_time_s":7680,"filament_mm":10870,"weight_g":32.9},"layers":0,"segments":0,"metadata":{"printable_area":"0x0,256x0,256x256,0x256"},"plates":[{"name":"Plate 1","facts":{"dimensions_mm":[110,96,40]},"slicer":{}},{"name":null,"facts":{"dimensions_mm":[30,30,16]},"slicer":{}}]}}"#;

#[test]
fn a_ready_reply_parses_its_facts_slicer_plates_and_unknowns() {
    let reply = wire::parse(READY).unwrap();
    assert_eq!((reply.slot.as_str(), reply.token), ("look", 7));
    let State::Ready { file, thumbnail, data } = reply.state else { panic!("ready") };
    assert_eq!((file.as_str(), thumbnail.as_str()), ("/c/KEY.3d", "/c/KEY-all.png"));
    assert_eq!(data.format, "3mf");
    assert_eq!(data.facts.dimensions_mm, Some([110., 96., 40.]));
    assert_eq!((data.facts.volume_cm3, data.facts.objects, data.facts.plates), (None, Some(4), Some(2)));
    assert_eq!(data.slicer.printer.as_deref(), Some("Bambu Lab P1S"));
    assert_eq!(data.slicer.nozzle_mm, vec![0.4]);
    assert_eq!(data.plates.len(), 2);
    assert_eq!(data.plates[1].name, None);
    assert_eq!(bed_from_metadata(data.metadata.iter().map(|(k, v)| (k.as_str(), v.as_str()))), Some([256., 256.]));
}

#[test]
fn failed_and_cancelled_replies_keep_only_what_the_view_needs() {
    let failed = wire::parse(r#"{"t":"model3d","slot":"look","token":3,"state":"failed","message":"This file cannot be previewed.","reason":"flea: truncated ASCII STL solid"}"#).unwrap();
    assert_eq!(failed.state, State::Failed { reason: "flea: truncated ASCII STL solid".into() });
    let cancelled = wire::parse(r#"{"t":"model3d","slot":"look","token":4,"state":"cancelled"}"#).unwrap();
    assert_eq!((cancelled.token, cancelled.state), (4, State::Cancelled));
}

#[test]
fn malformed_replies_are_refused() {
    for bad in [
        "", "not json", r#"{"t":"rows","slot":"look","token":1,"state":"cancelled"}"#,
        r#"{"t":"model3d","token":1,"state":"cancelled"}"#,
        r#"{"t":"model3d","slot":"look","state":"cancelled"}"#,
        r#"{"t":"model3d","slot":"look","token":-1,"state":"cancelled"}"#,
        r#"{"t":"model3d","slot":"look","token":1.5,"state":"cancelled"}"#,
        r#"{"t":"model3d","slot":"look","token":1,"state":"ready","data":{}}"#,
        r#"{"t":"model3d","slot":"look","token":1,"state":"ready","file":"rel.3d","data":{}}"#,
        r#"{"t":"model3d","slot":"look","token":1,"state":"ready","file":"/c/a.3d","data":[]}"#,
        r#"{"t":"model3d","slot":"look","token":1,"state":"done"}"#,
    ] {
        assert!(wire::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_column_facts_of_a_sliced_3mf_are_the_boards_two_groups() {
    let i = english();
    let State::Ready { data, .. } = wire::parse(READY).unwrap().state else { panic!() };
    let file = FileFacts { kind: "3MF document", size: "1.2 MB", modified: "2025-06-15 10:00" };
    let rows = facts::rows(&i, file, &data, 0);
    let pairs = |g: &Vec<(String, String)>| g.iter().map(|(a, b)| format!("{a}={b}")).collect::<Vec<_>>();
    assert_eq!(pairs(&rows[0]), ["Kind=3MF document", "Size=1.2 MB", "Modified=2025-06-15 10:00", "Dimensions=110.0 × 96.0 × 40.0 mm", "Objects=4", "Plates=2"]);
    assert_eq!(pairs(&rows[1]), ["Printer=Bambu Lab P1S", "Filament=Bambu PLA Basic", "Layer height=0.20 mm", "Print time=2 h 08 min", "Weight=32.9 g"]);
    assert_eq!(facts::bar_fact(&i, &data, 0).as_deref(), Some("110.0 × 96.0 × 40.0 mm"));
    assert_eq!(facts::bar_fact(&i, &data, 1).as_deref(), Some("30.0 × 30.0 × 16.0 mm"));
    assert_eq!(facts::plate_words(&i, 2), ["Plate 1", "Plate 2"]);
    assert!(facts::plate_words(&i, 1).is_empty());
}

#[test]
fn a_mesh_shows_volume_surface_triangles_and_watertight_and_hides_unknowns() {
    let i = english();
    let mut data = wire::Data { format: "stl".into(), ..Default::default() };
    data.facts = wire::Facts { dimensions_mm: Some([60., 40., 40.]), volume_cm3: Some(23.456), surface_cm2: Some(110.04), triangles: Some(12_500), open_edges: Some(0), ..Default::default() };
    let file = FileFacts { kind: "STL model", size: "210.7 kB", modified: "2025-06-15 10:00" };
    let labels = |rows: &Vec<Vec<(String, String)>>| rows[0].iter().map(|(a, b)| format!("{a}={b}")).collect::<Vec<_>>();
    let rows = facts::rows(&i, file, &data, 0);
    assert_eq!(rows.len(), 1, "no slicer group without slicer facts");
    assert_eq!(&labels(&rows)[3..], ["Dimensions=60.0 × 40.0 × 40.0 mm", "Volume=23.5 cm³", "Surface=110.0 cm²", "Triangles=12,500", "Watertight=yes"]);
    data.facts.open_edges = Some(1);
    data.facts.volume_cm3 = None;
    assert_eq!(&labels(&facts::rows(&i, file, &data, 0))[3..], ["Dimensions=60.0 × 40.0 × 40.0 mm", "Surface=110.0 cm²", "Triangles=12,500", "Watertight=1 open edge"]);
    data.facts.open_edges = Some(12);
    assert!(labels(&facts::rows(&i, file, &data, 0)).contains(&"Watertight=12 open edges".to_string()));
    assert_eq!(facts::bar_fact(&i, &data, 0).as_deref(), Some("60.0 × 40.0 × 40.0 mm"));
}

#[test]
fn a_print_shows_its_slicer_facts_and_its_time_in_the_bar_and_never_a_missing_one_as_zero() {
    let i = english();
    let mut data = wire::Data { format: "gcode".into(), layers: 240, ..Default::default() };
    data.facts.dimensions_mm = Some([60., 31., 48.]);
    data.slicer = wire::Slicer { printer: Some("Original Prusa MK4".into()), print_time_s: Some(4320.), ..Default::default() };
    assert_eq!(facts::bar_fact(&i, &data, 0).as_deref(), Some("1 h 12 min"));
    let file = FileFacts { kind: "G-code", size: "3.6 MB", modified: "2025-06-15 10:00" };
    let rows = facts::rows(&i, file, &data, 0);
    assert_eq!(rows[0].len(), 4, "no mesh facts for a toolpath");
    assert_eq!(rows[1], [("Printer".to_string(), "Original Prusa MK4".to_string()), ("Print time".into(), "1 h 12 min".into())]);
    data.slicer.print_time_s = None;
    assert_eq!(facts::bar_fact(&i, &data, 0).as_deref(), Some("60.0 × 31.0 × 48.0 mm"));
}

#[test]
fn times_layers_and_sizes_format_like_the_boards() {
    let i = english();
    assert_eq!(facts::print_time(&i, 4320.).as_deref(), Some("1 h 12 min"));
    assert_eq!(facts::print_time(&i, 7680.).as_deref(), Some("2 h 08 min"));
    assert_eq!(facts::print_time(&i, 2700.).as_deref(), Some("45 min"));
    assert_eq!(facts::print_time(&i, 29.).as_deref(), Some("0 min"));
    assert_eq!(facts::print_time(&i, -1.), None);
    assert_eq!(facts::print_time(&i, f64::NAN), None);
    assert_eq!(facts::layer_label(&i, 128, 240, 25.6), "Layer 128 of 240 · 25.60 mm");
    assert_eq!(facts::size_mm(&i, [60., 40., 40.]), "60.0 × 40.0 × 40.0 mm");
    assert_eq!(facts::cannot_preview(&i), "This file cannot be previewed.");
}

#[test]
fn tones_step_by_ten_percent_and_the_shade_is_darker_in_every_theme() {
    assert_eq!(color::tone_step(1.0), 1.0);
    assert_eq!(color::tone_step(-0.3), 0.0);
    assert_eq!(color::tone_step(0.325), 0.5);
    for fg in [[0.85, 0.78, 0.65], [0.2, 0.2, 0.25], [1.0, 0.42, 0.07]] {
        let ink = Ink::from_lit(fg);
        assert!(color::oklab(ink.shade)[0] < color::oklab(ink.lit)[0], "{fg:?}");
        let mid = ink.tone(0.325);
        assert_eq!(mid, color::mix(ink.shade, ink.lit, 0.5));
    }
    let round = color::from_oklab(color::oklab([0.3, 0.6, 0.9]));
    assert!(round.iter().zip([0.3, 0.6, 0.9]).all(|(a, b)| (a - b).abs() < 1e-3), "{round:?}");
    assert_eq!(color::to_bytes(color::from_bytes([255, 106, 19])), [255, 106, 19]);
}
