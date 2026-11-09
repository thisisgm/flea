use super::wire::{parse, read, request, Answer, Line};

#[test]
fn the_request_escapes_the_query_and_names_its_id() {
    assert_eq!(
        request(7, ".PNG"),
        "{\"c\":\"filter\",\"query\":\".PNG\",\"id\":7}\n"
    );
    assert_eq!(
        request(8, "a\"b\\c"),
        "{\"c\":\"filter\",\"query\":\"a\\\"b\\\\c\",\"id\":8}\n"
    );
}

#[test]
fn the_backends_own_answer_lines_parse_as_they_are_documented() {
    let line =
        r#"{"t":"filtered","id":7,"query":".PNG","n":1300,"rows":[0,2,4],"ms":0.012,"listing":1}"#;
    assert_eq!(
        parse(line),
        Ok(Some(Answer {
            id: 7,
            query: ".PNG".into(),
            listing: 1,
            total: 1300,
            exact: Some(vec![0, 2, 4]),
            close: vec![]
        }))
    );
    let clear = r#"{"t":"filtered","id":8,"query":"","n":3,"rows":null,"ms":0.001,"listing":2}"#;
    assert_eq!(
        parse(clear).unwrap().unwrap().exact,
        None,
        "an empty query answers null, the identity"
    );
    let close = r#"{"t":"filtered","id":9,"query":"ntoes","n":9,"rows":[],"close":[6],"ms":0.1,"listing":3}"#;
    assert_eq!(
        parse(close).unwrap().unwrap().close,
        vec![6],
        "the Filter040 producer's close rows ride beside an empty list"
    );
    assert_eq!(
        parse(r#"{"t":"rows","start":0,"listing":1,"rows":[]}"#),
        Ok(None),
        "another reply is not the filter's"
    );
}

#[test]
fn a_malformed_answer_is_refused_whole() {
    let bad = [
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[0,3],"listing":1}"#,
            "a row past the listing",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[2,1],"listing":1}"#,
            "rows out of the backend's order",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[1,1],"listing":1}"#,
            "a repeated row",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":["1"],"listing":1}"#,
            "a row that is not an index",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[-1],"listing":1}"#,
            "a negative row",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"listing":1}"#,
            "no rows at all",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"","n":3,"rows":[0],"listing":1}"#,
            "an empty query that narrows",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":null,"listing":1}"#,
            "a query answered with the identity",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[0],"close":[1],"listing":1}"#,
            "close rows beside exact ones",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[],"close":[1,1],"listing":1}"#,
            "a repeated close row",
        ),
        (
            r#"{"t":"filtered","id":-1,"query":"a","n":3,"rows":[],"listing":1}"#,
            "an id that is no request's",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[]}"#,
            "no listing stamp",
        ),
        (
            r#"{"t":"filtered","id":1,"query":7,"n":3,"rows":[],"listing":1}"#,
            "a query that is not text",
        ),
        (
            r#"{"t":"filtered","id":1,"query":"a","n":3,"rows":[],"listing":1"#,
            "a cut line",
        ),
    ];
    for (line, why) in bad {
        assert!(parse(line).is_err(), "{why} must be refused: {line}");
    }
}

fn reply(id: u64, rows: usize) -> String {
    let mut line =
        format!("{{\"t\":\"filtered\",\"id\":{id},\"query\":\"i\",\"n\":{rows},\"rows\":[");
    for row in 0..rows {
        line.push_str(if row == 0 { "" } else { "," });
        line.push_str(&row.to_string());
    }
    line + "],\"ms\":1.000,\"listing\":1}"
}

#[test]
fn an_answer_of_100k_rows_reads_in_a_few_allocations_and_a_stale_one_in_none() {
    let line = reply(4, 100_000);
    let before = crate::list::bench::allocations();
    let answer = parse(&line).unwrap().unwrap();
    let cost = crate::list::bench::allocations() - before;
    assert_eq!(answer.exact.as_ref().map(Vec::len), Some(100_000));
    assert!(
        cost <= 4,
        "the rows read straight into one list, not one string each: {cost} allocations"
    );
    let before = crate::list::bench::allocations();
    assert_eq!(
        read(&line, Some(5)),
        Ok(Line::Stale),
        "an answer to an older request is refused by its id"
    );
    assert_eq!(
        crate::list::bench::allocations() - before,
        0,
        "before a single row is read"
    );
    assert_eq!(
        read(&line, None),
        Ok(Line::Stale),
        "and so is any answer while nothing is asked"
    );
    assert!(
        matches!(read(&line, Some(4)), Ok(Line::Answer(_))),
        "the live one lands"
    );
    assert_eq!(read(r#"{"t":"rows","start":0}"#, Some(4)), Ok(Line::Other));
}
