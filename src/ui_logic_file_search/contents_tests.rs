use super::*;

#[test]
fn m9_g2rows_01_contents_envelope_is_shared_by_gui_and_tui() {
    let line = r#"{"t":"contentsmatches","gen":7,"listing":2,"start":0,"matches":[{"row":0,"path":"raw-�.txt","pathBytes":"7261772dff2e747874","lineNo":3,"line":"needle","operable":false}]}"#;
    let Some(Line::ContentsMatches(matches)) = parse_line(line).unwrap() else {
        panic!("shared contents parser")
    };
    assert_eq!(matches.matches[0].path_bytes, b"raw-\xff.txt");
    assert!(!matches.matches[0].operable);
    assert_eq!(
        parse_line(&matches.line()).unwrap(),
        Some(Line::ContentsMatches(matches))
    );
    assert!(
        !Wire::default().contents,
        "producer does not enable an unmounted GUI"
    );
}

#[test]
fn m9_g2rows_01_contents_parser_refuses_operable_raw_and_escaped_scope() {
    for (path, bytes) in [("raw-�.txt", "7261772dff2e747874"), ("../x", "2e2e2f78")] {
        let line = format!(
            r#"{{"t":"contentsmatches","gen":1,"listing":1,"start":0,"matches":[{{"row":0,"path":"{path}","pathBytes":"{bytes}","lineNo":1,"line":"needle","operable":true}}]}}"#
        );
        assert!(parse_line(&line).is_err());
    }
}
