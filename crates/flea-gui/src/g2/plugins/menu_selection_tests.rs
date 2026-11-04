use super::*;

#[gpui::test]
fn plugin_menu_runtime_mime_uses_only_captured_names_beyond_the_held_viewport(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (mut plugin, runtime) = plugin("checksums", &root);
    let block = &mut plugin
        .manifest
        .blocks
        .iter_mut()
        .find(|(key, _)| key == "menuActions")
        .unwrap()
        .1;
    if let Json::Arr(actions) = block {
        if let Json::Obj(fields) = &mut actions[0] {
            if let Json::Obj(when) =
                &mut fields.iter_mut().find(|(key, _)| key == "when").unwrap().1
            {
                when.push(("mime".into(), Json::Arr(vec![Json::Str("text/*".into())])));
            }
        }
    }
    let mut fixture = open(cx, root.clone(), plugin);
    let requests = Rc::new(RefCell::new(Vec::new()));
    let received = requests.clone();
    update(cx, fixture.handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::list::pane::Held {
            rows: vec![host.pane.held.rows[0].clone()],
            ..*host.pane.held
        });
        assert!(host.pane.held.row(2).is_none());
        shell.menu.plugin_runtime.dispatch = Some(Box::new(move |_, request| {
            received.borrow_mut().push(request);
            Ok(Json::Null)
        }));
    });
    let id = fixture.menu(cx, 0, vec![0, 2]);
    let shown = |cx: &mut TestAppContext| {
        fixture
            .entries(cx)
            .iter()
            .any(|row| row.action == "example.checksums:sha256")
    };
    assert!(!shown(cx), "missing selected facts stay closed");
    let snapshot = |id: u64, names: Vec<&str>| {
        Json::Obj(vec![
            ("t".into(), Json::Str("menuaction".into())),
            ("op".into(), Json::Str("snapshot".into())),
            ("id".into(), Json::Num(id.to_string())),
            ("ok".into(), Json::Bool(true)),
            (
                "kinds".into(),
                Json::Arr(vec![Json::Num("32768".into()); 2]),
            ),
            (
                "names".into(),
                Json::Arr(
                    names
                        .into_iter()
                        .map(|name| Json::Str(name.into()))
                        .collect(),
                ),
            ),
        ])
    };
    fixture.feed(cx, snapshot(id + 1, vec!["notes.txt", "other.txt"]));
    assert!(!shown(cx), "stale names do not populate another capture");
    fixture.feed(cx, snapshot(id, vec!["notes.txt"]));
    assert!(
        !shown(cx),
        "a partial names list does not guess the missing MIME"
    );
    fixture.feed(cx, snapshot(id, vec!["notes.txt", "other.txt"]));
    assert!(
        shown(cx),
        "the off-viewport selected text file matches without a stat or scan"
    );
    let first = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    let paths = [root.join("notes.txt"), root.join("other.txt")];
    fixture.validated(cx, &first, &paths);
    let second = fixture
        .submit_input(cx, CHECKSUM)
        .expect("the leased MIME facts survive the input card");
    assert_eq!(first.get("id"), second.get("id"));
    fixture.validated(cx, &second, &paths);
    assert_eq!(requests.borrow().len(), 1);
    assert_eq!(
        requests.borrow()[0].get("paths"),
        Some(&Json::Arr(
            paths
                .iter()
                .map(|path| Json::Str(path.to_string_lossy().into()))
                .collect()
        ))
    );
    runtime.shutdown();
}
