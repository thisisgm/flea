use super::facts::{self, FileFacts};
use super::tests::{english, READY};
use super::wire::{self, State};

#[test]
fn the_column_facts_of_a_sliced_3mf_are_the_boards_two_groups() {
    let i = english();
    let State::Ready { data, .. } = wire::parse(READY).unwrap().state else {
        panic!()
    };
    let file = FileFacts {
        kind: "3MF document",
        size: "1.2 MB",
        modified: "2025-06-15 10:00",
    };
    let rows = facts::rows(&i, file, &data, 0);
    let pairs = |g: &Vec<(String, String)>| {
        g.iter()
            .map(|(a, b)| format!("{a}={b}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        pairs(&rows[0]),
        [
            "Kind=3MF document",
            "Size=1.2 MB",
            "Modified=2025-06-15 10:00",
            "Dimensions=110.0 × 96.0 × 40.0 mm",
            "Objects=4",
            "Plates=2"
        ]
    );
    assert_eq!(
        pairs(&rows[1]),
        [
            "Printer=Bambu Lab P1S",
            "Filament=Bambu PLA Basic",
            "Layer height=0.20 mm",
            "Print time=2 h 08 min",
            "Weight=32.9 g"
        ]
    );
    assert_eq!(
        facts::bar_fact(&i, &data, 0).as_deref(),
        Some("110.0 × 96.0 × 40.0 mm")
    );
    assert_eq!(
        facts::bar_fact(&i, &data, 1).as_deref(),
        Some("30.0 × 30.0 × 16.0 mm")
    );
    assert_eq!(facts::plate_words(&i, 2), ["Plate 1", "Plate 2"]);
    assert!(facts::plate_words(&i, 1).is_empty());
}

#[test]
fn a_mesh_shows_volume_surface_triangles_and_watertight_and_hides_unknowns() {
    let i = english();
    let mut data = wire::Data {
        format: "stl".into(),
        ..Default::default()
    };
    data.facts = wire::Facts {
        dimensions_mm: Some([60., 40., 40.]),
        volume_cm3: Some(23.456),
        surface_cm2: Some(110.04),
        triangles: Some(12_500),
        open_edges: Some(0),
        ..Default::default()
    };
    let file = FileFacts {
        kind: "STL model",
        size: "210.7 kB",
        modified: "2025-06-15 10:00",
    };
    let labels = |rows: &Vec<Vec<(String, String)>>| {
        rows[0]
            .iter()
            .map(|(a, b)| format!("{a}={b}"))
            .collect::<Vec<_>>()
    };
    let rows = facts::rows(&i, file, &data, 0);
    assert_eq!(rows.len(), 1, "no slicer group without slicer facts");
    assert_eq!(
        &labels(&rows)[3..],
        [
            "Dimensions=60.0 × 40.0 × 40.0 mm",
            "Volume=23.5 cm³",
            "Surface=110.0 cm²",
            "Triangles=12,500",
            "Watertight=yes"
        ]
    );
    data.facts.open_edges = Some(1);
    data.facts.volume_cm3 = None;
    assert_eq!(
        &labels(&facts::rows(&i, file, &data, 0))[3..],
        [
            "Dimensions=60.0 × 40.0 × 40.0 mm",
            "Surface=110.0 cm²",
            "Triangles=12,500",
            "Watertight=1 open edge"
        ]
    );
    data.facts.open_edges = Some(12);
    assert!(
        labels(&facts::rows(&i, file, &data, 0)).contains(&"Watertight=12 open edges".to_string())
    );
    assert_eq!(
        facts::bar_fact(&i, &data, 0).as_deref(),
        Some("60.0 × 40.0 × 40.0 mm")
    );
}

#[test]
fn a_print_shows_its_slicer_facts_and_its_time_in_the_bar_and_never_a_missing_one_as_zero() {
    let i = english();
    let mut data = wire::Data {
        format: "gcode".into(),
        layers: 240,
        ..Default::default()
    };
    data.facts.dimensions_mm = Some([60., 31., 48.]);
    data.slicer = wire::Slicer {
        printer: Some("Original Prusa MK4".into()),
        print_time_s: Some(4320.),
        ..Default::default()
    };
    assert_eq!(facts::bar_fact(&i, &data, 0).as_deref(), Some("1 h 12 min"));
    let file = FileFacts {
        kind: "G-code",
        size: "3.6 MB",
        modified: "2025-06-15 10:00",
    };
    let rows = facts::rows(&i, file, &data, 0);
    assert_eq!(rows[0].len(), 4, "no mesh facts for a toolpath");
    assert_eq!(
        rows[1],
        [
            ("Printer".to_string(), "Original Prusa MK4".to_string()),
            ("Print time".into(), "1 h 12 min".into())
        ]
    );
    data.slicer.print_time_s = None;
    assert_eq!(
        facts::bar_fact(&i, &data, 0).as_deref(),
        Some("60.0 × 31.0 × 48.0 mm")
    );
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
    assert_eq!(
        facts::layer_label(&i, 128, 240, 25.6),
        "Layer 128 of 240 · 25.60 mm"
    );
    assert_eq!(facts::size_mm(&i, [60., 40., 40.]), "60.0 × 40.0 × 40.0 mm");
    assert_eq!(facts::cannot_preview(&i), "This file cannot be previewed.");
}
