use super::*;
use super::testdata::*;

#[test]
fn gcode_common_non_motion_commands_keep_geometry() {
    let mut failures = Vec::new();
    for command in ["G28 W", "G29 G", "EXCLUDE_OBJECT_START NAME=part_1", "TIMELAPSE_TAKE_FRAME", "TURN_OFF_HEATERS"] {
        let source = format!("{command}\nG90\nM83\nG1 X10 E1 F1200\n");
        for format in ["gcode", "bgcode"] {
            let bytes = if format == "gcode" { source.as_bytes().to_vec() } else { bgcode(0, 0, source.as_bytes(), source.len()) };
            if let Err(reason) = parse(format, &bytes) { failures.push(format!("{format} {command}: {reason}")); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[test]
fn polygon_repeated_corner_keeps_valid_mesh() {
    let source = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nf 1 2 3 3\n";
    assert!(parse("obj", source).is_ok(), "{:?}", parse("obj", source));
}

#[test]
fn empty_component_dag_has_total_expansion_bound() {
    let mut last = String::new();
    for depth in [10, 14, 18] {
        let mut source = String::from("<model><resources>");
        for id in 1..depth {
            source.push_str(&format!("<object id='{id}'><components><component objectid='{}'/><component objectid='{}'/></components></object>", id + 1, id + 1));
        }
        source.push_str(&format!("<object id='{depth}'><components/></object></resources><build><item objectid='1'/></build></model>"));
        let bytes = archive(&[("3D/3dmodel.model", source.as_bytes())]);
        let start = std::time::Instant::now();
        last = parse("3mf", &bytes).unwrap_err();
        eprintln!("DAG depth={depth} calls={} elapsed_us={} result={last}", (1usize << depth) - 1, start.elapsed().as_micros());
    }
    assert!(last.contains("excessive"), "component expansion reached no budget: {last}");
}

#[test]
fn lowercase_metadata_preserves_plate_name() {
    let files = [("metadata/model_settings.config".into(), b"<config><plate><metadata key='plater_name' value='Custom'/></plate></config>".to_vec())].into();
    let mut model = Model::default();
    projectmeta::read(&files, &mut model).unwrap();
    assert_eq!(model.plates.first().map(|p| p.name.as_str()), Some("Custom"));
}

#[test]
fn relationship_target_case_matches_part() {
    let files = zip::unpack(&testdata::project()).unwrap();
    let rels = b"<Relationships><Relationship Type='http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel' Target='/3D/3DModel.model'/></Relationships>";
    let mut entries: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
    entries.push(("_rels/.rels", rels));
    assert!(parse("3mf", &archive(&entries)).is_ok());
}

#[test]
fn optional_broken_xml_keeps_valid_geometry() {
    let files = zip::unpack(&testdata::project()).unwrap();
    let broken = b"<broken>";
    let mut entries: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
    entries.push(("Metadata/extra.xml", broken));
    assert!(parse("3mf", &archive(&entries)).is_ok(), "optional metadata: {:?}", parse("3mf", &archive(&entries)));
}

#[test]
fn sparse_slice_uses_plate_identity() {
    let plates = b"<config><plate><metadata key='plater_id' value='1'/></plate><plate><metadata key='plater_id' value='2'/></plate><plate><metadata key='plater_id' value='3'/></plate></config>";
    let slice = b"<config><plate><metadata key='index' value='2'/><metadata key='prediction' value='120'/><filament type='PLA' used_g='3' used_m='2'/></plate></config>";
    let files = [("Metadata/model_settings.config".into(), plates.to_vec()), ("Metadata/slice_info.config".into(), slice.to_vec())].into();
    let mut model = Model::default();
    projectmeta::read(&files, &mut model).unwrap();
    assert_eq!(model.plates[1].metadata.get("print_time").map(String::as_str), Some("120"));
    assert!(!model.plates[0].metadata.contains_key("print_time"));
}

#[test]
fn no_plate_slice_keeps_root_prediction() {
    let files = zip::unpack(&testdata::project()).unwrap();
    let root = std::str::from_utf8(&files["3D/3dmodel.model"]).unwrap().replace("<resources>", "<metadata name='prediction'>60</metadata><resources>");
    let mut entries: Vec<(&str, &[u8])> = files.iter().filter(|(p, _)| p.as_str() != "3D/3dmodel.model" && p.as_str() != "Metadata/model_settings.config").map(|(p, b)| (p.as_str(), b.as_slice())).collect();
    entries.push(("3D/3dmodel.model", root.as_bytes()));
    entries.push(("Metadata/slice_info.config", b"<config/>"));
    let model = parse("3mf", &archive(&entries)).unwrap();
    assert_eq!(model.slicer(None).print_time_s, Some(60.));
}

#[test]
fn optional_broken_json_keeps_valid_geometry() {
    let files = zip::unpack(&testdata::project()).unwrap();
    let mut entries: Vec<(&str, &[u8])> = files.iter().map(|(p,b)| (p.as_str(),b.as_slice())).collect();
    entries.push(("Metadata/extra.json",b"{\"broken\":"));
    assert!(parse("3mf",&archive(&entries)).is_ok());
}

#[test]
fn required_plate_metadata_and_optional_resource_limits_are_refused() {
    for (path,bytes) in [
        ("Metadata/model_settings.config",b"<broken>".to_vec()),
        ("Metadata/slice_info.config",b"{\"broken\":".to_vec()),
        ("Metadata/extra.xml",format!("{}{}","<n>".repeat(65),"</n>".repeat(65)).into_bytes()),
        ("Metadata/extra.json",format!("{{\"key\":{}0{}}}","[".repeat(200),"]".repeat(200)).into_bytes()),
    ] {
        let files = [(path.into(),bytes)].into();
        assert!(projectmeta::read(&files,&mut Model::default()).is_err(),"{path}");
    }
}

#[test]
fn equivalent_part_aliases_are_refused() {
    let files = zip::unpack(&testdata::project()).unwrap();
    let mut entries: Vec<(&str,&[u8])> = files.iter().map(|(p,b)| (p.as_str(),b.as_slice())).collect();
    entries.push(("3D/3DModel.model",&files["3D/3dmodel.model"]));
    assert!(parse("3mf",&archive(&entries)).unwrap_err().contains("ambiguous 3MF part"));
}

#[test]
fn degenerate_ply_faces_do_not_drop_valid_faces() {
    let header = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nelement face 3\nproperty list uchar int vertex_indices\nend_header\n";
    let source = format!("{header}0 0 0\n1 0 0\n0 1 0\n2 0 0\n3 0 1 2\n4 0 1 2 2\n3 0 1 3\n");
    assert_eq!(parse("ply",source.as_bytes()).unwrap().triangles.len(),2);
}

#[test]
fn reordered_and_duplicate_slice_identities_are_checked() {
    let settings = b"<config><plate><metadata key='plater_id' value='1'/></plate><plate><metadata key='plater_id' value='2'/></plate></config>";
    let slice = b"<config><plate><metadata key='index' value='2'/><metadata key='prediction' value='120'/><filament type='PETG' used_g='3' used_m='2'/></plate><plate><metadata key='index' value='1'/><metadata key='prediction' value='60'/></plate></config>";
    let files = [("metadata/model_settings.config".into(),settings.to_vec()),("Metadata/slice_info.config".into(),slice.to_vec())].into();
    let mut model = Model::default();
    projectmeta::read(&files,&mut model).unwrap();
    assert_eq!(model.slicer(Some(0)).print_time_s,Some(60.));
    assert_eq!(model.slicer(Some(1)).print_time_s,Some(120.));
    assert_eq!(model.slicer(Some(1)).weight_g,Some(3.));
    assert_eq!(model.slicer(Some(1)).filament_mm,Some(2000.));
    for info in ["<metadata key='index' value='1'/><metadata key='index' value='2'/>","<metadata key='index' value='0'/>","<metadata key='index' value='129'/>","<metadata key='index' value='2'/><metadata key='plater_id' value='1'/>"] {
        let files = [("Metadata/slice_info.config".into(),format!("<config><plate>{info}</plate></config>").into_bytes())].into();
        assert!(projectmeta::read(&files,&mut Model::default()).is_err());
    }
    let duplicate = b"<config><plate><metadata key='index' value='2'/></plate><plate><metadata key='index' value='2'/></plate></config>";
    let files = [("Metadata/slice_info.config".into(),duplicate.to_vec())].into();
    assert!(projectmeta::read(&files,&mut Model::default()).is_err());
}

#[test]
fn vendor_macro_skips_do_not_hide_malformed_motion_words() {
    for command in ["Xnan","Tbogus","G1 XNaN","G1 X1_2","G NaN","Fbogus"] {
        let source = format!("G1 X10 F1200\n{command}\n");
        assert!(parse("gcode",source.as_bytes()).is_err(),"{command}");
        let binary = bgcode(0,0,source.as_bytes(),source.len());
        assert!(parse("bgcode",&binary).is_err(),"{command}");
    }
}

// Signed area of each triangle in the z = 0 plane, positive for the counter-clockwise winding the faces below use.
fn plane_areas(model: &Model) -> Vec<f64> {
    model.triangles.iter().map(|t| {
        let [a,b,c] = t.vertices.map(|v| model.vertices[v as usize].position.map(f64::from));
        ((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]))/2.
    }).collect()
}

#[test]
fn polygon_revisited_corners_keep_their_outline() {
    let faces: [(&str,&[u8],f64); 3] = [
        ("keyhole",b"v 0 0 0\nv 4 0 0\nv 4 4 0\nv 0 4 0\nv 1 1 0\nv 1 3 0\nv 3 3 0\nv 3 1 0\nf 1 2 3 4 1 5 6 7 8 5\n",12.),
        ("pinch",b"v 0 0 0\nv 2 0 0\nv 2 2 0\nv 0 0 0\nv -2 0 0\nv -1 -2 0\nf 1 2 3 4 5 6\n",4.),
        ("three lobes",b"v 0 0 0\nv 2 0 0\nv 2 1 0\nv -1 2 0\nv -2 2 0\nv -1 -2 0\nv 0 -2 0\nf 1 2 3 1 4 5 1 6 7\n",3.),
    ];
    for (name,source,outline) in faces {
        let model = parse("obj",source).unwrap_or_else(|reason| panic!("{name}: {reason}"));
        let areas = plane_areas(&model);
        assert!(areas.iter().all(|&a| a > 0.),"{name}: a triangle left the face winding: {areas:?}");
        let covered: f64 = areas.iter().sum();
        assert!((covered-outline).abs() < 1e-9,"{name}: triangles cover {covered} mm2 where the outline holds {outline}");
    }
}

#[test]
fn gcode_slicer_start_lines_keep_geometry() {
    let start = ["M862.3 P \"MK4\" ; printer model check","M862.6 P\"Input shaper\"","M115 U5.1.2+13478","M117 Hello","M486 AShape-Box",
        "M555 X95 Y76 W60 H60","M17 ; enable steppers","M82 ;absolute extrusion mode","M117 Printing...","M205 X10 Y10","T0",
        "M1002 gcode_claim_action : 2","M73.2   R1.0","G29.1 Z0","M620 S0A","M9833.2","G392 S0","G380 S2 Z-25 F300","T255","N4 M862.3 P \"MK4\"*61"];
    let mut failures = Vec::new();
    for line in start {
        let source = format!("{line}\nG90\nM83\nG1 X10 E1 F1200\n");
        for format in ["gcode","bgcode"] {
            let bytes = if format == "gcode" { source.as_bytes().to_vec() } else { bgcode(0,0,source.as_bytes(),source.len()) };
            match parse(format,&bytes) {
                Ok(model) if model.segments.len() == 1 => {}
                other => failures.push(format!("{format} {line}: {:?}",other.map(|m| m.segments.len()))),
            }
        }
    }
    assert!(failures.is_empty(),"{}",failures.join("; "));
}

#[test]
fn optional_xml_with_a_control_character_keeps_valid_geometry() {
    let files = zip::unpack(&testdata::project()).unwrap();
    let mut entries: Vec<(&str,&[u8])> = files.iter().map(|(p,b)| (p.as_str(),b.as_slice())).collect();
    entries.push(("Metadata/extra.xml",b"<a>\x0c</a>"));
    let parsed = parse("3mf",&archive(&entries));
    assert!(parsed.is_ok(),"optional metadata with a control byte: {:?}",parsed.err());
    let required = [("Metadata/model_settings.config".into(),b"<config>\x0c</config>".to_vec())].into();
    let refused = projectmeta::read(&required,&mut Model::default()).err();
    assert!(refused.as_deref().is_some_and(|e| e.contains("control")),"required plate metadata with a control byte: {refused:?}");
}
