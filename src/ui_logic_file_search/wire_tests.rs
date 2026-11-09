// The session's request lines and its reply lines in docs/protocol.md's shapes, malformed lines refused.
use super::*;

#[test]
fn requests_write_the_protocols_lines() {
    let open = Request::Open { scope: "/home/u".into(), folder: "/home/u/a \"q\"".into(), hidden: true };
    assert_eq!(open.line(), r#"{"c":"searchopen","path":"/home/u","folder":"/home/u/a \"q\"","hidden":true}"#);
    let query = Request::Query { query: "tag:Red\nx".into(), gen: 3, count: 40, sizes: 12 };
    assert_eq!(query.line(), r#"{"c":"searchquery","query":"tag:Red\nx","gen":3,"count":40,"sizes":12}"#);
    assert_eq!(Request::Cancel.line(), r#"{"c":"searchcancel"}"#);
    assert_eq!(Request::Close.line(), r#"{"c":"searchclose"}"#);
    for line in [open.line(), query.line()] {
        crate::jsondoc::parse(&line).expect("every request is one JSON object");
    }
}

#[test]
fn reply_lines_parse_and_others_pass_through() {
    let searched = r#"{"t":"searched","n":11,"scanned":1040,"ms":12.400,"cancelled":false,"gen":7,"walking":true}"#;
    assert_eq!(
        parse_line(searched),
        Ok(Some(Line::Searched { n: 11, scanned: 1040, ms: 12.4, cancelled: false, gen: 7, walking: true, partial: None }))
    );
    let walk = r#"{"t":"searched","n":6252,"scanned":45807,"ms":48.711,"cancelled":true}"#;
    assert_eq!(
        parse_line(walk),
        Ok(Some(Line::Searched { n: 6252, scanned: 45807, ms: 48.711, cancelled: true, gen: 0, walking: false, partial: None }))
    );
    assert_eq!(
        parse_line(r#"{"t":"searching","n":812,"scanned":41200,"ms":300.114}"#),
        Ok(Some(Line::Searching { n: 812, scanned: 41200, ms: 300.114 }))
    );
    assert_eq!(parse_line(r#"{"t":"searchopened","path":"/s"}"#), Ok(Some(Line::Opened { path: "/s".into() })));
    assert_eq!(parse_line(r#"{"t":"searchclosed","path":"/s"}"#), Ok(Some(Line::Closed { path: "/s".into() })));
    assert_eq!(parse_line(r#"{"t":"listed","n":3,"path":"/d","gen":2}"#), Ok(None));
}

#[test]
fn malformed_search_lines_are_refused_never_guessed() {
    for line in [
        r#"{"t":"searched","n":-1,"scanned":0,"ms":0,"cancelled":false}"#,
        r#"{"t":"searched","n":1,"scanned":0,"ms":"x","cancelled":false}"#,
        r#"{"t":"searched","n":1,"scanned":0,"ms":1,"cancelled":"yes"}"#,
        r#"{"t":"searched","n":1,"scanned":0,"ms":1,"cancelled":false,"gen":"7"}"#,
        r#"{"t":"searched","n":1,"scanned":0,"ms":1,"cancelled":false,"gen":1.5}"#,
        r#"{"t":"searching","n":1}"#,
        r#"{"t":"searchopened"}"#,
        r#"{"n":1}"#,
        r#"{"t":"searched""#,
        "",
    ] {
        assert!(parse_line(line).is_err(), "{line}");
    }
}

#[test]
fn a_partial_answer_keeps_its_reason_and_a_complete_one_has_none() {
    let base = r#"{"t":"searched","n":1,"scanned":9,"ms":1.000,"cancelled":false,"gen":2,"walking":false"#;
    let line = |tail: &str| format!("{base}{tail}}}");
    let answer = |partial: Option<&str>| {
        Ok(Some(Line::Searched { n: 1, scanned: 9, ms: 1., cancelled: false, gen: 2, walking: false, partial: partial.map(Into::into) }))
    };
    assert_eq!(parse_line(&line(r#","partial":true,"partial_reason":"unreadable""#)), answer(Some("unreadable")));
    assert_eq!(parse_line(&line(r#","partial":false,"partial_reason":"""#)), answer(None));
    assert_eq!(parse_line(&line("")), answer(None), "an older producer's line names no partial");
    for tail in [r#","partial":true"#, r#","partial":true,"partial_reason":"""#, r#","partial":"yes","partial_reason":"x""#] {
        assert!(parse_line(&line(tail)).is_err(), "{tail}");
    }
}
