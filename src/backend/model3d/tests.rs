use super::*;
use super::testdata::*;

#[test]
fn only_supported_extensions_select_a_parser() {
    assert_eq!(supported("MODEL.STL"),Some("stl"));
    assert_eq!(supported("model.gcode"),Some("gcode"));
    assert_eq!(supported("obj"),None);
    assert_eq!(supported("code.rs"),None);
}

#[test]
fn stl_binary_named_solid_and_ascii_match() {
    let ascii = b"solid triangle\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 10 0 0\nvertex 0 10 0\nendloop\nendfacet\nendsolid triangle\n";
    let mut binary = vec![0;80];
    binary[..5].copy_from_slice(b"solid");
    binary.extend_from_slice(&1u32.to_le_bytes());
    for n in [0f32,0.,1.,0.,0.,0.,10.,0.,0.,0.,10.,0.] { binary.extend_from_slice(&n.to_le_bytes()) }
    binary.extend_from_slice(&0u16.to_le_bytes());
    let a = parse("stl",ascii).unwrap();
    let b = parse("stl",&binary).unwrap();
    assert_eq!(a.vertices,b.vertices);
    assert_eq!(a.facts(None),b.facts(None));
    assert_eq!(a.facts(None).surface_cm2,Some(0.5));
    assert_eq!(a.facts(None).open_edges,Some(3));
    assert_eq!(a.facts(None).volume_cm3,None);
}

#[test]
fn obj_vertex_colors_and_negative_concave_faces() {
    let model = parse("obj",b"o concave\nv 0 0 0 1 0 0\nv 2 0 0\nv 2 2 0\nv 1 1 0\nv 0 2 0\nusemtl red\nf -5 -4 -3 -2 -1\n").unwrap();
    assert_eq!(model.triangles.len(),3);
    assert_eq!(model.vertices[0].color,Some([255,0,0,255]));
    assert!((model.facts(None).surface_cm2.unwrap()-0.03).abs() < 1e-9);
    let mut bytes = Vec::new();
    wire::write(&model,&mut bytes).unwrap();
    assert_eq!(wire::read(bytes.as_slice()).unwrap(),model);
    let alpha = parse("obj",b"v 0 0 0 1 0 0 0.5\nv 1 0 0 0 255 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    assert_eq!(alpha.vertices[0].color,Some([255,0,0,128]));
    assert_eq!(alpha.vertices[1].color,Some([0,255,0,255]));
}

#[test]
fn tetrahedron_facts_use_mm_cm_and_welded_topology() {
    let model = parse("obj",OBJ).unwrap();
    let facts = model.facts(None);
    assert_eq!(facts.dimensions_mm,[10.,10.,10.]);
    assert_eq!(facts.open_edges,Some(0));
    assert!((facts.volume_cm3.unwrap()-1./6.).abs() < 1e-9);
    assert!((facts.surface_cm2.unwrap()-(150.+50.*3f64.sqrt())/100.).abs() < 1e-9);
    let mut open = model.clone();
    open.triangles.pop();
    open.objects[0].triangle_count -= 1;
    assert_eq!(open.facts(None).open_edges,Some(3));
    assert_eq!(open.facts(None).volume_cm3,None);
}

#[test]
fn ply_ascii_and_both_binary_orders_keep_vertex_colors() {
    let ascii = parse("ply",&ply("ascii")).unwrap();
    for format in ["binary_little_endian","binary_big_endian"] {
        let binary = parse("ply",&ply(format)).unwrap();
        assert_eq!(ascii,binary, "{format}");
    }
    assert_eq!(ascii.vertices[2].color,Some([0,0,255,255]));
    assert_eq!(ascii.facts(None).dimensions_mm,[10.,10.,0.]);
}

#[test]
fn amf_materials_vertex_colors_and_constellation_transforms() {
    let model = parse("amf",AMF.as_bytes()).unwrap();
    assert_eq!(model.objects.len(),2);
    assert_eq!(model.vertices[0].position,[5.,0.,0.]);
    assert_eq!(model.vertices[0].color,Some([0,255,0,255]));
    assert_eq!(model.triangles[0].colors[0],Some([0,255,0,255]));
    assert_eq!(model.triangles[0].colors[1],Some([255,0,0,255]));
    assert_eq!(model.materials[0].name,"PLA");
    assert_eq!(model.objects[1].transform[9],20.);
    assert!((model.vertices[5].position[0]-10.).abs() < 0.0001);
    let compressed = archive(&[("object.amf",AMF.as_bytes())]);
    assert_eq!(parse("amf",&compressed).unwrap(),model);
}

#[test]
fn project_reads_every_plate_external_part_material_and_slicer_setting() {
    let model = parse("3mf",&project()).unwrap();
    assert_eq!(model.plates.len(),2);
    assert_eq!(model.objects.len(),2);
    assert_eq!(model.objects[1].plate,1);
    assert_eq!(model.vertices[4].position,[30.,0.,0.]);
    assert_eq!(model.materials[0].name,"PLA & red");
    assert_eq!(model.triangles[0].colors,[Some([255,0,0,255]);3]);
    assert_eq!(model.facts(Some(1)).dimensions_mm,[10.,10.,10.]);
    assert_eq!(model.facts(None).dimensions_mm,[40.,10.,10.]);
    assert_eq!(model.facts(None).open_edges,Some(0));
    assert_eq!(model.metadata["printer"],"P1S");
    assert_eq!(model.metadata["filament"],"PLA");
    assert_eq!(model.metadata["nozzle_mm"],"0.4");
    assert_eq!(model.metadata["layer_height_mm"],"0.2");
    assert_eq!(model.thumbnails[0].bytes,PNG);
    let mut encoded = Vec::new();
    wire::write(&model,&mut encoded).unwrap();
    assert_eq!(wire::read(encoded.as_slice()).unwrap(),model);
    let summary = crate::jsondoc::parse(&wire::summary(&model)).unwrap();
    assert_eq!(summary.get("plates").unwrap().as_array().unwrap().len(),2);
}

#[test]
fn independently_deflated_3mf_and_material_combinations_parse() {
    let model = parse("3mf",include_bytes!("fixtures/deflated.3mf")).unwrap();
    assert_eq!(model.facts(None).dimensions_mm,[10.,10.,0.]);
    let doc = xml::parse("<resources><basematerials id='1'/><compositematerials id='3' matid='1'><composite values='0.5 0.5'/></compositematerials><multiproperties id='4' pids='3'><multi pindices='0'/></multiproperties></resources>").unwrap();
    let mut model = Model {
        materials: vec![Material { name: "red".into(), color: Some([255,0,0,255]) }, Material { name: "blue".into(), color: Some([0,0,255,255]) }],
        ..Model::default()
    };
    let mut groups = std::collections::BTreeMap::from([(1,vec![0,1])]);
    properties::resolve(&doc,doc.root,&mut groups,&mut model).unwrap();
    assert_eq!(model.materials[groups[&3][0] as usize].color,Some([128,0,128,255]));
    assert_eq!(model.materials[groups[&4][0] as usize].color,Some([128,0,128,255]));
}

#[test]
fn gcode_embedded_png_is_kept_and_bad_base64_is_refused() {
    let thumbnail = "; thumbnail begin 1x1 92\n; iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR42mNgAAAAAgAB5Sfe/AAAAABJRU5ErkJggg==\n; thumbnail end\n";
    let source = format!("{thumbnail}{}",std::str::from_utf8(GCODE).unwrap());
    let model = parse("gcode",source.as_bytes()).unwrap();
    assert_eq!(model.thumbnails[0].bytes,PNG);
    assert!(parse("gcode",source.replace("iVBOR","iV!OR").as_bytes()).is_err());
}

#[test]
fn wire_flush_failure_is_reported_before_publication() {
    struct RefuseFlush;
    impl std::io::Write for RefuseFlush {
        fn write(&mut self,bytes: &[u8]) -> std::io::Result<usize> { Ok(bytes.len()) }
        fn flush(&mut self) -> std::io::Result<()> { Err(std::io::Error::other("disk full")) }
    }
    assert_eq!(wire::write(&parse("obj",OBJ).unwrap(),RefuseFlush).unwrap_err(),"disk full");
}

#[test]
fn compressed_stream_mutations_remain_bounded_without_panics() {
    let original = hex("edc73701c020000030adec96e9ffc30012922f21a65c6afbfe3ee6da27a8aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabe7a01");
    for at in 0..original.len() {
        for byte in [0,255,127] {
            let mut mutated = original.clone();
            mutated[at] = byte;
            assert!(std::panic::catch_unwind(|| codecs::inflate(&mutated,15000,false)).is_ok());
            assert!(std::panic::catch_unwind(|| codecs::heatshrink(&mutated,1024,11)).is_ok());
        }
    }
    for n in 0..original.len() { assert!(codecs::inflate(&original[..n],15000,false).is_err()); }
}

#[test]
fn parser_and_facts_measurement_ten_thousand_binary_triangles() {
    let mut binary = vec![0;80];
    binary.extend_from_slice(&10000u32.to_le_bytes());
    for _ in 0..10000 {
        for n in [0f32,0.,1.,0.,0.,0.,10.,0.,0.,0.,10.,0.] { binary.extend_from_slice(&n.to_le_bytes()) }
        binary.extend_from_slice(&0u16.to_le_bytes());
    }
    let mut times = Vec::new();
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let model = parse("stl",&binary).unwrap();
        assert_eq!(model.facts(None).triangles,10000);
        times.push(started.elapsed().as_secs_f64()*1000.);
    }
    times.sort_by(f64::total_cmp);
    eprintln!("MEASURE model3d debug STL parse+facts: {:.3}..{:.3} ms, 3 runs, 10000 triangles, {} input bytes",times[0],times[2],binary.len());
}

#[test]
fn prusa_orca_and_cura_metadata_share_canonical_facts() {
    for (slicer,config,printer,filament,height) in [
        ("PrusaSlicer","printer_model = MK4\nfilament_type = PETG\nlayer_height = 0.15\n","MK4","PETG","0.15"),
        ("OrcaSlicer","machine_name = A1\nmaterial_type = PLA\nlayer_height = 0.2\n","A1","PLA","0.2"),
        ("Cura","machine_name = Ender\nmaterial = ABS\nlayer_height = 0.3\n","Ender","ABS","0.3"),
    ] {
        let mut model = Model::default();
        projectmeta::pairs(&mut model,&format!("; generated by {slicer}\n{config}"));
        assert_eq!(model.metadata["slicer"],slicer);
        assert_eq!(model.metadata["printer"],printer);
        assert_eq!(model.metadata["filament"],filament);
        assert_eq!(model.metadata["layer_height_mm"],height);
    }
}

#[test]
fn gcode_preserves_travel_extrusion_resets_and_real_layers_without_z_hops() {
    let model = parse("gcode",GCODE).unwrap();
    assert_eq!(model.layers.len(),2);
    assert_eq!(model.layers[0].z_mm,0.2);
    assert_eq!(model.layers[1].z_mm,0.4);
    assert_eq!(model.segments.iter().filter(|s| s.extrusion_mm > 0.).count(),5);
    assert_eq!(model.segments.last().unwrap().end,[40.,0.,0.4]);
    assert_eq!(model.metadata["print_time"],"1h 2m 3s");
    assert_eq!(model.metadata["weight_g"],"2.1");
    assert_eq!(model.facts(None).volume_cm3,None);
    let mut encoded = Vec::new();
    wire::write(&model,&mut encoded).unwrap();
    assert_eq!(wire::read(encoded.as_slice()).unwrap(),model);
}

#[test]
fn gcode_relative_units_tools_and_helical_arcs() {
    let model = parse("gcode",b"G20\nG90\nM82\nG1X1Y0E1F60\nG21\nG91\nM83\nG1X1E2\nT1\nG1X1E1\nG90\nM83\nG3X7.4Y0I-10J0Z1E3\n").unwrap();
    assert_eq!(model.segments[0].end,[25.4,0.,0.]);
    assert_eq!(model.segments[1].end,[26.4,0.,0.]);
    assert_eq!(model.segments[2].tool,1);
    assert!(model.segments.len() > 30);
    assert_eq!(model.segments.last().unwrap().end,[7.4,0.,1.]);
    assert!((model.segments[3..].iter().map(|s| f64::from(s.extrusion_mm)).sum::<f64>()-3.).abs() < 1e-5);
}

#[test]
fn deflate_fixed_stored_dynamic_and_zlib_match_independent_vectors() {
    let payload = b"G90\nM83\nG1 Z0.2 F1200\nG1 X10 E1\n";
    for compressed in ["73b734e0f2b530e67237548832d03352703334323000f1220c0d145c0db900", "012000dfff4739300a4d38330a4731205a302e322046313230300a4731205831302045310a"] {
        assert_eq!(codecs::inflate(&hex(compressed),32,false).unwrap(),payload);
    }
    let zlib = hex("789c73b734e0f2b530e67237548832d03352703334323000f1220c0d145c0db90067f50623");
    assert_eq!(codecs::inflate(&zlib,32,true).unwrap(),payload);
    let encoded = bgcode(1,0,&zlib,32);
    assert_eq!(parse("bgcode",&encoded).unwrap().segments,parse("gcode",payload).unwrap().segments);
    assert!(codecs::inflate(&zlib,31,true).is_err());
    assert!(codecs::inflate(&zlib,MAX_EXPANDED+1,true).is_err());
    let dynamic = hex("edc73701c020000030adec96e9ffc30012922f21a65c6afbfe3ee6da27a8aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabe7a01");
    assert_eq!(codecs::inflate(&dynamic,15000,false).unwrap(),b"abcdefghijklmno".repeat(1000));
    assert_eq!(codecs::heatshrink(&hex("a0800100"),4,11).unwrap(),b"AAAA");
    assert_eq!(codecs::heatshrink(&hex("a0800080"),4,12).unwrap(),b"AAAA");
}

#[test]
fn bgcode_heatshrink_and_meatpack_keep_movement_and_reject_checksums() {
    for compression in [0,2,3] {
        let payload = if compression == 0 { GCODE.to_vec() } else { heatshrink_literal(GCODE) };
        let mut encoded = bgcode(compression,0,&payload,GCODE.len());
        assert_eq!(parse("bgcode",&encoded).unwrap().segments,parse("gcode",GCODE).unwrap().segments);
        *encoded.last_mut().unwrap() ^= 1;
        assert!(parse("bgcode",&encoded).is_err());
    }
    // Independent packed bytes: enable, G1 X10, escape Y, then newline and reset.
    let packed = [255,255,251,0x1d,0xeb,0x01,0x0f,b'Y',0xcc,255,255,249];
    assert_eq!(bgcode::meat_for_test(&packed).unwrap(),b"G1 X10Y0\n");
    for encoding in [1,2] {
        let encoded = bgcode(0,encoding,&packed,packed.len());
        assert_eq!(parse("bgcode",&encoded).unwrap().segments.last().unwrap().end,[10.,0.,0.]);
    }
    assert!(bgcode::meat_for_test(&[255,255]).is_err());
}

#[test]
fn xml_entities_and_component_cycles_fail_closed() {
    assert!(xml::parse("<!DOCTYPE model [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><model>&x;</model>").is_err());
    assert!(xml::parse("<model><x></model>").is_err());
    assert!(xml::parse("<model x='1' x='2'/>").is_err());
    let cyclic = b"<model><resources><object id='1'><components><component objectid='1'/></components></object></resources><build><item objectid='1'/></build></model>";
    assert!(parse("3mf",&archive(&[("3D/3dmodel.model",cyclic)])).unwrap_err().contains("cyclic"));
    assert!(zip::unpack(&archive(&[("../escape",b"bad")])).is_err());
}

#[test]
fn each_parser_refuses_bad_indices_huge_counts_and_nonfinite_coordinates() {
    for (format,bytes) in [
        ("obj",b"v NaN 0 0\nf 1 1 1\n".as_slice()),
        ("obj",b"v 0 0 0\nf 1 2 99999999999\n"),
        ("ply",b"ply\nformat ascii 1.0\nelement vertex 18446744073709551615\nend_header\n"),
        ("amf",b"<amf><object id='1'><mesh><vertices/><volume><triangle><v1>0</v1><v2>1</v2><v3>2</v3></triangle></volume></mesh></object></amf>"),
        ("gcode",b"G1 X999999999999999999999999999999\n"),
        ("gcode",b"G20\nM83\nG1 X1 E1000000000 F60\n"),
        ("bgcode",b"GCDE\x01\0\0\0\0\0\x01\0\x01\0\xff\xff\xff\xff\x01\0\0\0\0\0a"),
    ] { assert!(parse(format,bytes).is_err(),"{format}") }
}

#[test]
fn deterministic_hostile_and_truncated_corpus_never_panics() {
    let mut binary = vec![0;80];
    binary.extend_from_slice(&1u32.to_le_bytes());
    for n in [0f32,0.,1.,0.,0.,0.,10.,0.,0.,0.,10.,0.] { binary.extend_from_slice(&n.to_le_bytes()) }
    binary.extend_from_slice(&0u16.to_le_bytes());
    let corpus = [("obj",OBJ.to_vec()),("ply",ply("binary_big_endian")),("amf",AMF.as_bytes().to_vec()),
        ("3mf",project()),("gcode",GCODE.to_vec()),("bgcode",bgcode(0,0,GCODE,GCODE.len())),
        ("stl",binary),("ply",ply("ascii")),("ply",ply("binary_little_endian")),("3mf",include_bytes!("fixtures/deflated.3mf").to_vec()),
        ("bgcode",bgcode(2,0,&heatshrink_literal(GCODE),GCODE.len())),
        ("stl",b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n".to_vec())];
    for (format,bytes) in corpus {
        for n in 0..bytes.len() {
            assert!(std::panic::catch_unwind(|| parse(format,&bytes[..n])).is_ok(),"{format}: truncated at {n}");
        }
        for seed in 0..128usize {
            let mut hostile = bytes.clone();
            for step in 0..8 { let at = (seed*7919+step*17)%hostile.len(); hostile[at] = [0,255,b'<',b'&',b'9',b'-',b'\n',b'\x7f'][step] }
            assert!(std::panic::catch_unwind(|| parse(format,&hostile)).is_ok(),"{format}: mutation {seed}");
        }
    }
    for n in 0..256usize {
        let bytes: Vec<u8> = (0..n).map(|i| ((i*31+n*17)%256) as u8).collect();
        for format in ["stl","3mf","obj","ply","amf","gcode","bgcode"] {
            assert!(std::panic::catch_unwind(|| parse(format,&bytes)).is_ok(),"{format}: random {n}");
        }
    }
}
