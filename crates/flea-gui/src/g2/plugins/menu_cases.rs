use super::*;

#[gpui::test]
fn plugin_menu_runtime_action_is_mounted_and_the_real_executable_receives_only_the_validated_selection(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    let mut fixture = open(cx, root.clone(), plugin);
    let received = Rc::new(RefCell::new(Vec::new()));
    let answers = received.clone();
    let run = runtime.clone();
    update(cx, fixture.handle, |shell, _, _| {
        shell.menu.plugin_runtime.dispatch = Some(Box::new(move |id, request| {
            assert_eq!(id, "example.checksums");
            let reply = run.request(request)?;
            answers.borrow_mut().push(reply.message.clone());
            Ok(reply.message)
        }));
    });
    fixture.menu(cx, 1, vec![0]);
    let entries = fixture.entries(cx);
    let at = entries
        .iter()
        .position(|r| r.action == "example.checksums:sha256")
        .expect("the real mounted card has the declared action");
    assert_eq!(entries[at].glyph, Some("hash"));
    let (key, svg) = crate::g2::context_menus::glyphs::mark("hash");
    assert!(
        crate::g2::context_menus::glyphs::known("hash")
            && key == "flea-menu:hash"
            && std::str::from_utf8(svg).unwrap().contains("M4 9h16"),
        "the row draws mn2's hash cut, not a blank lead"
    );
    assert!(
        crate::g2::plugin_settings::glyphs::glyph("hash").is_some(),
        "the plugin's Settings row draws the same mark"
    );
    assert!(entries[..at].iter().any(|row| row.id == "compress"));
    assert!(
        entries[at + 1].separator,
        "plugin actions end the archive group"
    );
    assert!(!entries[at].disabled);
    let validate = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    assert!(
        received.borrow().is_empty(),
        "no path reaches the executable before selection validation"
    );
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert!(
        received.borrow().is_empty(),
        "the native prompt sends no run"
    );
    let revalidate = fixture.submit_input(cx, CHECKSUM).unwrap();
    assert_eq!(
        revalidate.get("id"),
        validate.get("id"),
        "the same capture is revalidated"
    );
    assert!(
        received.borrow().is_empty(),
        "submit still waits for fresh validation"
    );
    fixture.validated(cx, &revalidate, &[root.join("notes.txt")]);
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert_eq!(
        received.borrow().len(),
        1,
        "duplicate validation never repeats an action"
    );
    assert_eq!(
        received.borrow()[0].get("message").and_then(Json::as_str),
        Some("Checksum matches.")
    );
    let receipt = std::fs::read_to_string(root.join("checksums/requests.jsonl")).unwrap();
    let requests = receipt
        .lines()
        .map(|line| jsondoc::parse(line).unwrap())
        .collect::<Vec<_>>();
    let run = requests
        .iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("run"))
        .unwrap();
    assert_eq!(
        run.get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );
    assert!(
        run.get("folder").is_none() && run.get("path").is_none() && run.get("cwd").is_none(),
        "selection-only actions receive no unselected folder or path field"
    );
    assert!(run
        .as_object()
        .unwrap()
        .iter()
        .all(|(key, _)| ["op", "action", "paths", "settings", "input"].contains(&key.as_str())));
    assert_eq!(
        run.get("input")
            .and_then(|v| v.get("checksum"))
            .and_then(Json::as_str),
        Some(CHECKSUM)
    );
    assert!(
        !receipt.contains("photo.png"),
        "the unselected cursor file is private"
    );
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_mime_all_items_disabled_failed_and_hidden_are_live(cx: &mut TestAppContext) {
    let root = scratch();
    let (mut plugin, runtime) = plugin("checksums", &root);
    plugin.manifest.blocks.iter_mut().find(|(k, _)| k == "menuActions").unwrap().1 = jsondoc::parse(
        r#"[{"id":"sha256","label":"Verify Checksum…","glyph":"hash","when":{"mime":["text/*"],"minItems":1,"maxItems":2,"folders":false}}]"#).unwrap();
    let mut fixture = open(cx, root, plugin);
    let shown = |fixture: &Fixture, cx: &mut TestAppContext| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "example.checksums:sha256")
    };
    fixture.menu(cx, 0, vec![0]);
    assert!(shown(&fixture, cx));
    fixture.menu(cx, 1, vec![1]);
    assert!(!shown(&fixture, cx), "image MIME does not match text/*");
    fixture.menu(cx, 0, vec![0, 1]);
    assert!(!shown(&fixture, cx), "every selected item must match");
    fixture.menu(cx, 0, vec![0]);
    update(cx, fixture.handle, |shell, window, cx| {
        shell
            .menu
            .plugin_runtime
            .plugins
            .last_mut()
            .unwrap()
            .enabled = false;
        shell.provider_refresh(window, cx);
    });
    assert!(!shown(&fixture, cx));
    update(cx, fixture.handle, |shell, window, cx| {
        let plugin = shell.menu.plugin_runtime.plugins.last_mut().unwrap();
        plugin.enabled = true;
        plugin.failure = Some(manifest::Invalid::new("start", "fixture failed"));
        shell.provider_refresh(window, cx);
    });
    assert!(!shown(&fixture, cx));
    update(cx, fixture.handle, |shell, window, cx| {
        shell
            .menu
            .plugin_runtime
            .plugins
            .last_mut()
            .unwrap()
            .failure = None;
        shell.launch.settings.data =
            jsondoc::parse(r#"{"menu":{"hidden":["example.checksums"]}}"#).unwrap();
        shell.provider_refresh(window, cx);
    });
    assert!(!shown(&fixture, cx));
    assert!(fixture.choose(cx, "example.checksums:sha256", "").is_none());
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_send_targets_mount_and_dispatch_to_the_real_executable(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (plugin, runtime) = plugin("send-target", &root);
    let targets = runtime
        .request(jsondoc::parse(r#"{"op":"targets"}"#).unwrap())
        .unwrap()
        .message;
    let mut fixture = open(cx, root.clone(), plugin);
    let requests = Rc::new(RefCell::new(Vec::new()));
    let received = requests.clone();
    let run = runtime.clone();
    update(cx, fixture.handle, |shell, _, _| {
        shell
            .menu
            .plugin_runtime
            .targets
            .insert("example.send-target".into(), targets);
        shell.menu.plugin_runtime.dispatch = Some(Box::new(move |id, request| {
            assert_eq!(id, "example.send-target");
            let reply = run.request(request.clone())?;
            received.borrow_mut().push(request);
            Ok(reply.message)
        }));
    });
    fixture.menu(cx, 1, vec![0]);
    let entries = fixture.entries(cx);
    let row = entries
        .iter()
        .find(|r| r.action == "plugin-send:example.send-target")
        .expect("declared send row on the mounted card");
    let leaves = row.submenu.as_ref().unwrap();
    assert_eq!(leaves.len(), 2);
    assert!(!leaves[0].disabled);
    assert!(leaves[1].disabled && leaves[1].errored);
    assert!(
        leaves.iter().all(|r| r.hint.is_none()),
        "no state word widens the menu"
    );
    let validate = fixture
        .choose(cx, "plugin-send:example.send-target", "receiver")
        .unwrap();
    assert!(requests.borrow().is_empty());
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert_eq!(requests.borrow().len(), 1);
    assert_eq!(
        requests.borrow()[0].get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert_eq!(
        requests.borrow().len(),
        1,
        "duplicate validation cannot replay a send"
    );
    fixture.menu(cx, 0, vec![0]);
    let moved = fixture
        .choose(cx, "plugin-send:example.send-target", "receiver")
        .unwrap();
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    fixture.validated(cx, &moved, &[root.join("notes.txt")]);
    assert_eq!(
        requests.borrow().len(),
        1,
        "a changed capture sends nothing"
    );
    let receipt = std::fs::read_to_string(root.join("send-target/requests.jsonl")).unwrap();
    assert!(receipt.contains("\"op\": \"send\""));
    assert!(!receipt.contains("photo.png"));
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_send_disabled_and_failed_plugins_remove_the_choice(cx: &mut TestAppContext) {
    let root = scratch();
    let (plugin, runtime) = plugin("send-target", &root);
    let mut fixture = open(cx, root, plugin);
    fixture.menu(cx, 0, vec![0]);
    let row = fixture
        .entries(cx)
        .into_iter()
        .find(|r| r.action == "plugin-send:example.send-target")
        .unwrap();
    assert!(row.disabled && row.errored && row.hint.is_none());
    update(cx, fixture.handle, |shell, window, cx| {
        shell
            .menu
            .plugin_runtime
            .plugins
            .last_mut()
            .unwrap()
            .enabled = false;
        shell.provider_refresh(window, cx);
    });
    assert!(!fixture
        .entries(cx)
        .iter()
        .any(|r| r.action == "plugin-send:example.send-target"));
    update(cx, fixture.handle, |shell, window, cx| {
        let plugin = shell.menu.plugin_runtime.plugins.last_mut().unwrap();
        plugin.enabled = true;
        plugin.failure = Some(manifest::Invalid::new("start", "failed"));
        shell.provider_refresh(window, cx);
    });
    assert!(!fixture
        .entries(cx)
        .iter()
        .any(|r| r.action == "plugin-send:example.send-target"));
    assert!(fixture
        .choose(cx, "plugin-send:example.send-target", "receiver")
        .is_none());
    runtime.shutdown();
}

// The row is already open on its glyph when the mark read starts, so only a redraw can change it.
fn send_row_before_and_after_its_mark_read(
    cx: &mut TestAppContext,
    svg: Option<&[u8]>,
) -> (Entry, Entry) {
    let root = scratch();
    let (mut plugin, runtime) = plugin("send-target", &root);
    runtime.shutdown();
    plugin.mark_path = root.join("send-target/mark.svg").to_string_lossy().into();
    if let Some(svg) = svg {
        std::fs::write(&plugin.mark_path, svg).unwrap();
    }
    let mut fixture = open(cx, root, plugin);
    fixture.menu(cx, 0, vec![0]);
    let row = |fixture: &Fixture, cx: &mut TestAppContext| {
        fixture
            .entries(cx)
            .into_iter()
            .find(|r| r.action == "plugin-send:example.send-target")
            .expect("declared send row on the open card")
    };
    let before = row(&fixture, cx);
    update(cx, fixture.handle, |shell, window, cx| {
        shell.load_send_marks(window, cx)
    });
    cx.run_until_parked();
    (before, row(&fixture, cx))
}

#[gpui::test]
fn plugin_menu_runtime_send_row_switches_to_its_mark_once_the_open_menu_reads_it(
    cx: &mut TestAppContext,
) {
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'/>";
    let (before, after) = send_row_before_and_after_its_mark_read(cx, Some(svg));
    assert!(before.mark.is_none(), "the row opens on its glyph");
    assert_eq!(before.glyph, Some("network"));
    let mark = after.mark.expect("the open row redraws with the read mark");
    assert!(mark.path.ends_with("send-target/mark.svg"));
    assert_eq!(&*mark.svg, &svg[..]);
}

#[gpui::test]
fn plugin_menu_runtime_send_row_keeps_its_glyph_when_the_mark_file_is_missing(
    cx: &mut TestAppContext,
) {
    let (_, after) = send_row_before_and_after_its_mark_read(cx, None);
    assert!(after.mark.is_none());
    assert_eq!(after.glyph, Some("network"));
}

#[gpui::test]
fn plugin_menu_runtime_send_row_keeps_its_glyph_when_the_mark_is_over_64_kib(
    cx: &mut TestAppContext,
) {
    let (_, after) = send_row_before_and_after_its_mark_read(cx, Some(&[b' '; 64 * 1024 + 1]));
    assert!(after.mark.is_none());
    assert_eq!(after.glyph, Some("network"));
}
