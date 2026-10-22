use super::*;

#[test]
fn list_paths_cannot_inject_another_line_or_field() {
    let path = "/files/a\"b\nnext\\file";
    let mut wire = Vec::new();
    Request::List {
        path,
        first: 20,
        select: Some(path),
    }
    .write_to(&mut wire)
    .unwrap();
    let text = String::from_utf8(wire).unwrap();
    assert_eq!(text.lines().count(), 1);
    let document = jsondoc::parse(&text).unwrap();
    assert_eq!(string(&document, "c").unwrap(), "list");
    assert_eq!(string(&document, "path").unwrap(), path);
    assert_eq!(string(&document, "select").unwrap(), path);
    assert_eq!(uint(&document, "first").unwrap(), 20);
    assert_eq!(document.get("hidden"), Some(&Json::Bool(false)));
    let mut wire = Vec::new();
    Request::List {
        path,
        first: 0,
        select: None,
    }
    .write_to(&mut wire)
    .unwrap();
    assert!(jsondoc::parse(std::str::from_utf8(&wire).unwrap())
        .unwrap()
        .get("select")
        .is_none());
}

#[test]
fn window_and_quit_keep_the_existing_wire_shape() {
    let mut wire = Vec::new();
    Request::Window {
        start: 17,
        count: 20,
    }
    .write_to(&mut wire)
    .unwrap();
    Request::Quit.write_to(&mut wire).unwrap();
    assert_eq!(
        wire,
        b"{\"c\":\"window\",\"start\":17,\"count\":20}\n{\"c\":\"quit\"}\n"
    );
}

#[test]
fn parses_the_initial_pair_and_preserves_escaped_names() {
    assert_eq!(
        parse_reply(r#"{"t":"listed","n":2,"path":"/files/"}"#).unwrap(),
        Reply::Listed {
            path: "/files/".into(),
            total: 2
        }
    );
    let line = r#"{"t":"rows","start":42,"rows":[{"n":"a\nb","d":false,"s":12,"m":-3,"p":33261,"i":"text-x-script","t":false,"k":0,"l":"../t\"x"},{"n":"caf\u00e9","d":true,"s":0,"m":1700000000,"p":16877,"i":"folder","t":true,"k":1,"v":2049,"x":false,"tg":["n"]}],"kinds":["Script","Folder"],"ms":0.5,"listing":9007199254740993}"#;
    let rows = vec![
        RowFacts {
            name: "a\nb".into(),
            size: 12,
            mtime: -3,
            mode: 33261,
            icon: "text-x-script".into(),
            link: Some("../t\"x".into()),
            real_modes: true,
            ..RowFacts::default()
        },
        RowFacts {
            name: "caf\u{e9}".into(),
            dir: true,
            mtime: 1700000000,
            mode: 16877,
            icon: "folder".into(),
            ..RowFacts::default()
        },
    ];
    assert_eq!(
        parse_reply(line).unwrap(),
        Reply::Rows {
            start: 42,
            rows,
            listing: 9007199254740993
        }
    );
    assert_eq!(
        parse_reply(r#"{"t":"rows","start":0,"rows":[],"listing":1}"#).unwrap(),
        Reply::Rows {
            start: 0,
            rows: vec![],
            listing: 1
        }
    );
}

#[test]
fn rejects_malformed_fields_and_never_uses_nested_envelope_fields() {
    for bad in [
        r#"{"rows":[{"t":"listed","n":2,"path":"/wrong"}]}"#,
        r#"{"t":"listed","n":-1,"path":"/files"}"#,
        r#"{"t":"listed","n":1.5,"path":"/files"}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":4}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":false,"s":-1,"m":0,"p":0,"i":"x"}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":0,"s":0,"m":0,"p":0,"i":"x"}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":false,"s":0,"m":0.5,"p":0,"i":"x"}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":false,"s":0,"m":0,"p":4294967296,"i":"x"}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":false,"s":0,"m":0,"p":0,"i":"x","l":1}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":false,"s":0,"m":0,"p":0,"i":"x","x":"no"}],"listing":1}"#,
        r#"{"t":"rows","start":0,"rows":[{"n":"a","d":false,"s":0,"m":0,"p":0}],"listing":1}"#,
        r#"{"t":"rows"}"#,
        "{",
    ] {
        assert!(parse_reply(bad).is_err(), "{bad}");
    }
    assert_eq!(
        parse_reply(r#"{"t":"changed","path":"/files"}"#).unwrap(),
        Reply::Other
    );
    assert_eq!(
        parse_reply(r#"{"t":"error","where":"scan","path":"/files","msg":"denied"}"#).unwrap(),
        Reply::Error {
            operation: "scan".into(),
            path: "/files".into(),
            message: "denied".into(),
            mode: 0,
        }
    );
}

#[test]
fn refused_directory_modes_survive_the_reply_parser() {
    for mode in [0, 0o40000, 0o40700] {
        let line = format!(
            r#"{{"t":"error","where":"scan","path":"/files","msg":"denied","mode":{mode}}}"#
        );
        assert!(
            matches!(parse_reply(&line).unwrap(), Reply::Error { mode: got, .. } if got == mode)
        );
    }
    for mode in ["-1", "1.5", "4294967296", "\"448\"", "null"] {
        let line = format!(
            r#"{{"t":"error","where":"scan","path":"/files","msg":"denied","mode":{mode}}}"#
        );
        assert!(parse_reply(&line).is_err(), "invalid mode {mode}");
    }
}

#[test]
fn a_write_failure_is_returned_to_the_caller() {
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        Request::Quit.write_to(&mut Closed).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}
