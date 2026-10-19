use super::*;

#[test]
fn r3_obj_groups_do_not_open_a_closed_object_or_inflate_object_count() {
    let plain = format!("o shell\n{}", std::str::from_utf8(testdata::OBJ).unwrap());
    let mut grouped = String::new();
    let mut group = 0;
    for line in plain.lines() {
        if line.starts_with("f ") {
            group += 1;
            grouped.push_str(&format!("g face_{group}\n"));
        }
        grouped.push_str(line);
        grouped.push('\n');
    }
    let control = parse("obj", plain.as_bytes()).unwrap();
    let actual = parse("obj", grouped.as_bytes()).unwrap();
    assert_eq!(actual.vertices, control.vertices);
    assert_eq!(actual.triangles, control.triangles);
    eprintln!("ungrouped facts: {:?}; grouped facts: {:?}", control.facts(None), actual.facts(None));
    let encoded = wire::summary(&actual);
    eprintln!("grouped summary: {encoded}");
    assert_eq!(actual.facts(None), control.facts(None), "face groups must not change a single object's physical facts");
}
