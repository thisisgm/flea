// Reserved folder-peek wire coverage for tab transfers.
use super::*;

#[test]
fn the_folder_check_goes_out_as_one_reserved_peek_and_its_answer_reads_back() {
    assert_eq!(
        peek_line("/tmp/a\"b", PEEK_FIRST, false, false),
        r#"{"c":"peek","path":"/tmp/a\"b","first":2,"hidden":false,"hiddenLast":false}"#,
        "the request carries the reserved discriminator and both flags off"
    );
    let answer = jsondoc::parse(r#"{"t":"peeked","path":"/tmp/a","n":0,"rows":[],"hidden":false,"hiddenLast":false,"first":2,"failed":true}"#).unwrap();
    let read = peeked(&answer).unwrap();
    assert_eq!(
        (
            read.path.as_str(),
            read.hidden,
            read.read_failed,
            read.hidden_last,
            read.first
        ),
        ("/tmp/a", false, true, false, 2),
        "a peeked line reads back its four discriminating fields"
    );
    assert_eq!(
        peeked(&jsondoc::parse(r#"{"t":"listed","path":"/tmp/a"}"#).unwrap()),
        None,
        "any other line is not a peek answer"
    );
    assert_eq!(
        peeked(&jsondoc::parse(r#"{"t":"peeked","path":"/tmp/a","first":-1}"#).unwrap()),
        None,
        "and a peek answer without a row count is refused"
    );
}
