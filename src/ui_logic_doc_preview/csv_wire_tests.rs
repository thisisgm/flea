use super::*;

// A raw reply reads each line as its text, an empty run as an empty line, and stops at a malformed tail.
#[test]
fn csv_wire_reads_raw_lines_and_only_code_replies() {
    let reply = r#"{"t":"code","token":7,"state":"plain","first":1,"total":1,"lines":[[0,"a,\"b\""]],"records":1,"firstRecord":1}"#;
    assert_eq!(parse_reply(reply).unwrap().lines, vec![r#"a,"b""#]);
    let reply = parse_reply(
        r#"{"t":"code","token":2,"state":"binary","first":0,"lines":[[0,"x"],[],[0,"y"],oops]}"#,
    )
    .unwrap();
    assert_eq!(
        (reply.state, reply.first, reply.lines),
        (
            Answer::Binary,
            1,
            vec!["x".into(), String::new(), "y".into()]
        )
    );
    assert_eq!(
        parse_reply(r#"{"t":"meta","token":1}"#),
        None,
        "another reply type is not a code reply"
    );
    let ask = Ask {
        record: 3,
        first: 5,
        count: 40,
    };
    assert_eq!(
        ask_line(9, "look", "/d/a \"b\".csv", '\t', &ask),
        r#"{"c":"code","token":9,"slot":"look","path":"/d/a \"b\".csv","first":5,"count":40,"raw":true,"csv":"\t","record":3}"#
    );
    assert!(
        Answer::from_wire("large").refused()
            && Answer::from_wire("plain").settled()
            && !Answer::Waiting.settled()
    );
}
