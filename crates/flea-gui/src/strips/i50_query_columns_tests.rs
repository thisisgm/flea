//! Composition remains mounted while the nested Columns folder filters its active rows.
use super::*;

#[gpui::test]
fn i50_query_filter_nested_columns(cx: &mut TestAppContext) {
    use crate::window::startup::View;
    use flea::protocol::Reply;
    let (handle, mut read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane = crate::pane::Pane::new("/files/nested".into(), 620.);
        host.pane.clock.use_manual();
        host.pane
            .receive(Reply::Listed {
                path: "/files/nested".into(),
                total: 1,
            })
            .unwrap();
        host.pane
            .receive(Reply::Rows {
                start: 0,
                listing: 3,
                rows: vec![flea::protocol::RowFacts {
                    name: "日本.txt".into(),
                    ..Default::default()
                }],
            })
            .unwrap();
        host.changed(&shell.locale);
        shell.set_view(View::Columns, window, cx);
        assert!(shell.filter_action("filter", window, cx));
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let paths = host.columns.as_ref().unwrap().area.drawn.clone();
        let first = host.pane.first_count();
        let folders = host.nav.folders.borrow();
        let (hidden, hidden_last) = (folders.hidden, folders.hidden_last);
        drop(folders);
        for path in paths {
            let name = if path == "/" { "files" } else { "nested" };
            let path = flea::json::escape(&path);
            let line = format!(
                r#"{{"t":"peeked","path":"{path}","first":{first},"n":1,"hidden":{hidden},"hiddenLast":{hidden_last},"failed":false,"mode":0,"rows":[{{"n":"{name}","d":true}}]}}"#
            );
            shell.receive(
                0,
                crate::backend::Event::Peeked(flea::jsondoc::parse(&line).unwrap()),
                window,
                cx,
            );
        }
    });
    frame(cx, handle);
    while read().is_some() {}
    let mut native = input(cx, handle);
    native.replace_and_mark_text_in_range(None, "日本", Some(2..2));
    frame(cx, handle);
    assert!(native.bounds_for_range(2..2).is_some());
    assert_eq!(read(), None);
    native.replace_text_in_range(None, "日本");
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("\"id\":1") && sent[0].contains("日本"));
    edit(cx, handle, |shell, _, cx| {
        assert!(shell.filter_line(0, r#"{"t":"filtered","id":1,"query":"日本","n":1,"rows":[0],"close":[],"ms":0.01,"listing":3}"#, cx));
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(shell.view, View::Columns);
        assert_eq!(host.path, "/files/nested");
        assert_eq!(host.filter.shown.as_deref(), Some([0].as_slice()));
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(crate::columns::geometry::Slot::Active)
            .unwrap()
            .borrow();
        assert_eq!(paint.visible().collect::<Vec<_>>(), [0]);
    });
}
