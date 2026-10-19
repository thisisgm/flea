//! The two backend lines the bar asks with and the two answers, as the backend writes them.
use flea_gui::path::{answer, jump_line, peek_line, Answer, Jumped, Peeked};

fn parsed(line: &str) -> Option<Answer> {
    answer(&flea::jsondoc::parse(line).unwrap())
}

#[test]
fn the_asks_escape_their_paths_and_name_the_ask_a_whole_one_follows() {
    assert_eq!(
        peek_line("/srv/a \"b\"", 12, true),
        "{\"c\":\"peek\",\"path\":\"/srv/a \\\"b\\\"\",\"first\":12,\"hidden\":true,\"hiddenLast\":false}"
    );
    assert_eq!(
        jump_line(3, 2, &["/a".into()], &["/b \"q\"".into()]),
        "{\"c\":\"jump\",\"id\":3,\"ranking\":2,\"favourites\":[\"/a\"],\"recent\":[\"/b \\\"q\\\"\"]}"
    );
    assert_eq!(
        jump_line(1, 0, &[], &[]),
        "{\"c\":\"jump\",\"id\":1,\"ranking\":0,\"favourites\":[],\"recent\":[]}"
    );
}

#[test]
fn a_peek_answer_keeps_only_directories_and_a_failed_read_keeps_its_mode() {
    let line = r#"{"t":"peeked","path":"/srv","hidden":false,"hiddenLast":false,"first":40,"n":3,"v":2049,"w":true,"rows":[{"n":"www","d":true,"i":"folder"},{"n":"notes.txt","d":false,"i":"text-plain"},{"n":"git","d":true,"i":"folder"}]}"#;
    let want = Peeked {
        path: "/srv".into(),
        dirs: vec!["www".into(), "git".into()],
        ..Peeked::default()
    };
    assert_eq!(parsed(line), Some(Answer::Peeked(want)));
    let refused = r#"{"t":"peeked","path":"/root","hidden":true,"hiddenLast":false,"first":40,"n":0,"failed":true,"mode":16832,"rows":[]}"#;
    let want = Peeked {
        path: "/root".into(),
        hidden: true,
        failed: true,
        mode: 0o40700,
        dirs: Vec::new(),
    };
    assert_eq!(parsed(refused), Some(Answer::Peeked(want)));
    assert_eq!(
        parsed(r#"{"t":"peeked","hidden":true}"#),
        None,
        "a peek with no path answers nothing"
    );
}

#[test]
fn a_jump_answer_carries_each_source_and_the_frecency_scores() {
    let line = r#"{"t":"jumped","id":7,"favourites":["/a"],"zoxide":["/b \"q\""],"recent":["/c"],"frecency":{"/a":12.5,"/b \"q\"":3},"ms":1.500}"#;
    let want = Jumped {
        id: 7,
        favourites: vec!["/a".into()],
        zoxide: vec!["/b \"q\"".into()],
        recent: vec!["/c".into()],
        frecency: vec![("/a".into(), 12.5), ("/b \"q\"".into(), 3.)],
    };
    assert_eq!(parsed(line), Some(Answer::Jumped(want)));
    assert_eq!(
        parsed(r#"{"t":"jumped","favourites":[]}"#),
        None,
        "a jump answer names its ask"
    );
    assert_eq!(
        parsed(r#"{"t":"located","path":"/x"}"#),
        None,
        "another reply is not the bar's"
    );
}
