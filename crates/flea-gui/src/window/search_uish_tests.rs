//! Search readers follow the mounted empty answer and preserve the typed query.
use super::*;

#[gpui::test]
fn mounted_empty_answer_keeps_query_case_and_does_not_stamp_a_result(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    for character in ["M", "i", "X", "e", "D"] {
        key(cx, handle, character, Some(character));
    }
    let generation = edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!(
            session.query, "MiXeD",
            "each typed character must retain its case before the answer"
        );
        assert_eq!(
            session.gen, 5,
            "five typed characters must send five query generations"
        );
        session.gen as usize
    });
    answer(cx, handle, 0, generation, None);
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.search.owner.session.query, "MiXeD");
        assert_eq!(host.search.owner.session.stamps.first_rows_at, 0);
        assert!(shell.search_empty(0, window, cx).is_some());
    });
    assert_eq!(call(cx, handle, "searchQuery").stdout.trim(), "MiXeD");
    assert_eq!(call(cx, handle, "searchMode").stdout.trim(), "typing");
    assert_eq!(
        call(cx, handle, "searchRect")
            .stdout
            .split_whitespace()
            .count(),
        4
    );
    assert!(call(cx, handle, "noMatchHeadline").stdout.contains("MiXeD"));
}
