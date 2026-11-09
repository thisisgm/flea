//! A settled contents result passes its actual match into the mounted Quick Look viewer.
use super::*;
use crate::g2::pv1::doc::View;

#[gpui::test]
fn vf_look_quick_look_centers_the_search_match_at_line_102(cx: &mut TestAppContext) {
    let (handle, mut requests) = contents_needle(cx);
    snapshot(cx, handle, 6, 16, "src/main.rs", true);
    reply(
        cx,
        handle,
        0,
        &matches(6, 16, "src/main.rs", true).replace("\"lineNo\":3", "\"lineNo\":102"),
    );
    results(cx, handle);
    key(cx, handle, "space", Some(" "));
    frame(cx, handle);
    let code = edit(cx, handle, |shell, _, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        let doc = look.subject().unwrap().body.doc().unwrap();
        match &doc.view {
            View::Code(code) => code.clone(),
            _ => panic!("code Quick Look"),
        }
    });
    let token = code.read_with(cx, |code, _| code.lines.token);
    assert!(
        std::iter::from_fn(&mut requests).any(|line| line.contains("\"c\":\"code\"")),
        "the mounted view asked the real backend channel"
    );
    let lines = (1..=164)
        .map(|i| {
            format!(
                "[0,\"{}\"]",
                if i == 102 {
                    "let needle = load();".to_owned()
                } else {
                    format!("let value_{i} = {i};")
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    reply(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"code","token":{token},"slot":"look","state":"plain","first":1,"total":164,"lines":[{lines}]}}"#
        ),
    );
    cx.run_until_parked();
    frame(cx, handle);
    code.read_with(cx, |code, _| {
        let rows = code.view().rows;
        assert!(
            code.top > 0 && code.top <= 101 && code.top + rows > 101,
            "line 102 must be visible, got top {} and {rows} rows",
            code.top + 1
        );
        assert!(
            (101. - code.top as f32 - rows as f32 / 2.).abs() <= 1.,
            "the match centers in the viewport"
        );
    });
}
