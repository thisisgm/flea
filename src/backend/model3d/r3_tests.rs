use super::*;

fn material_groups(explicit_object: bool) -> String {
    let mut source = if explicit_object { "o shell\n".to_string() } else { String::new() };
    let mut face = 0;
    for line in std::str::from_utf8(testdata::OBJ).unwrap().lines() {
        if line.starts_with("f ") {
            let (group,material) = [("shell_red","red"),("shell_blue","blue"),("shell_red accessory","red"),("","blue")][face];
            source.push_str(&format!("g {group}\nusemtl {material}\n"));
            face += 1;
        }
        source.push_str(line);
        source.push('\n');
    }
    source
}

fn without_groups(source: &str) -> String {
    source.lines().filter(|line| line.split_whitespace().next() != Some("g"))
        .map(|line| format!("{line}\n")).collect()
}

#[test]
fn obj_material_groups_keep_one_closed_object_and_triangle_materials_through_wire() {
    let source = material_groups(true);
    let expected = parse("obj",without_groups(&source).as_bytes()).unwrap();
    let actual = parse("obj",source.as_bytes()).unwrap();
    assert_eq!(actual.vertices,expected.vertices);
    assert_eq!(actual.triangles,expected.triangles);
    assert_eq!(actual.materials,expected.materials);
    assert_eq!(actual.objects,expected.objects);
    assert_eq!(actual.objects[0].name,"shell");
    assert_eq!(actual.objects[0].first_triangle,0);
    assert_eq!(actual.objects[0].triangle_count,4);
    assert_eq!(actual.materials.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),["red","blue"]);
    assert_eq!(actual.triangles.iter().map(|t| t.material).collect::<Vec<_>>(),[Some(0),Some(1),Some(0),Some(1)]);
    assert_eq!(actual.facts(None),expected.facts(None));
    assert_eq!(actual.facts(None).open_edges,Some(0));
    assert!((actual.facts(None).volume_cm3.unwrap()-1./6.).abs() < 1e-9);
    let summary = crate::jsondoc::parse(&wire::summary(&actual)).unwrap();
    let plate = &summary.get("plates").unwrap().as_array().unwrap()[0];
    for container in [&summary,plate] {
        let facts = container.get("facts").unwrap();
        assert_eq!(facts.get("objects").unwrap().as_f64(),Some(1.));
        assert_eq!(facts.get("open_edges").unwrap().as_f64(),Some(0.));
        assert!((facts.get("volume_cm3").unwrap().as_f64().unwrap()-1./6.).abs() < 1e-9);
    }
    let mut bytes = Vec::new();
    wire::write(&actual,&mut bytes).unwrap();
    assert_eq!(wire::read(bytes.as_slice()).unwrap(),actual);
}

#[test]
fn obj_groups_without_o_share_one_implicit_object() {
    let source = material_groups(false);
    let expected = parse("obj",without_groups(&source).as_bytes()).unwrap();
    let actual = parse("obj",source.as_bytes()).unwrap();
    assert_eq!(actual,expected);
    assert_eq!(actual.objects.len(),1);
    assert_eq!(actual.objects[0].name,"Object 1");
    assert_eq!(actual.objects[0].triangle_count,4);
    assert_eq!(actual.facts(None).open_edges,Some(0));
    assert!((actual.facts(None).volume_cm3.unwrap()-1./6.).abs() < 1e-9);
}

#[test]
fn obj_explicit_object_boundaries_keep_topology_separate_even_when_names_repeat() {
    let mut source = String::from("o shell\n");
    let mut face = 0;
    for line in std::str::from_utf8(testdata::OBJ).unwrap().lines() {
        if line.starts_with("f ") {
            if face == 2 { source.push_str("o shell\n") }
            source.push_str(&format!("g face_{face}\n"));
            face += 1;
        }
        source.push_str(line);
        source.push('\n');
    }
    let actual = parse("obj",source.as_bytes()).unwrap();
    let expected = parse("obj",without_groups(&source).as_bytes()).unwrap();
    assert_eq!(actual,expected);
    assert_eq!(actual.objects.len(),2);
    assert_eq!(actual.objects.iter().map(|o| (o.first_triangle,o.triangle_count)).collect::<Vec<_>>(),[(0,2),(2,2)]);
    assert_eq!(actual.facts(None).open_edges,Some(8));
    assert_eq!(actual.facts(None).volume_cm3,None);
}
