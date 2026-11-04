use super::*;

#[test]
fn u04_loading_rows_cannot_answer_from_retained_cache_and_hidden_size_is_empty() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    assert!(!ask(&mut cx, handle, "rowNameText", &["0"]).is_empty());
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = Rc::new(Held {
                listing: 2,
                total: 2000,
                start: 100,
                rows: bench::fixture("uniform")
                    .into_iter()
                    .skip(100)
                    .take(40)
                    .collect(),
            });
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert!(
                !list.rows.is_empty(),
                "the negative control lost its retained cache"
            );
            assert!(list
                .visible()
                .iter()
                .any(|shown| shown.index == 0 && shown.prepared.is_none()));
        })
        .unwrap();
    for name in ["rowNameText", "rowSizeText", "rowNameColor"] {
        assert_eq!(
            ask(&mut cx, handle, name, &["0"]),
            "",
            "a loading row answered {name} from an old frame"
        );
    }
    assert_eq!(ask(&mut cx, handle, "rowGlyph", &["0"]), "file");
    handle
        .update(&mut cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"columns":["mode","date"]}"#).unwrap();
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = bench::held(bench::fixture("uniform"));
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(ask(&mut cx, handle, "rowSizeText", &["0"]), "");
    assert!(!ask(&mut cx, handle, "rowNameText", &["0"]).is_empty());
}
