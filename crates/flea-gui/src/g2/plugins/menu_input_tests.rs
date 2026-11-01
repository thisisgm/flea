use super::*;

fn prompt(cx: &mut TestAppContext) -> (Fixture, Runtime, Rc<RefCell<Vec<Json>>>, Json) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    let mut fixture = open(cx, root.clone(), plugin);
    let requests = Rc::new(RefCell::new(Vec::new()));
    let received = requests.clone();
    update(cx, fixture.handle, |shell, _, _| {
        shell.menu.plugin_runtime.dispatch = Some(Box::new(move |_, request| {
            received.borrow_mut().push(request);
            Ok(Json::Null)
        }));
    });
    fixture.menu(cx, 0, vec![0]);
    let validate = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    update(cx, fixture.handle, |shell, _, _| {
        assert!(
            shell.plugins.input.is_some(),
            "the native field, Cancel and Verify surface is mounted"
        );
        assert_eq!(
            shell.menu.captured.as_ref().unwrap().id,
            validate.get("id").and_then(Json::as_f64).unwrap() as u64
        );
        assert!(
            !shell.menu_opened(),
            "the menu closes while its snapshot lease remains"
        );
    });
    cx.update_window(fixture.handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    assert!(fixture
        .drain()
        .iter()
        .all(|r| r.get("op").and_then(Json::as_str) != Some("close")));
    (fixture, runtime, requests, validate)
}

#[gpui::test]
fn plugin_menu_runtime_input_cancel_retires_its_original_capture_and_sends_no_run(
    cx: &mut TestAppContext,
) {
    let (mut fixture, runtime, requests, first) = prompt(cx);
    let closed = fixture.cancel_input(cx);
    assert!(closed
        .iter()
        .any(|r| r.get("op").and_then(Json::as_str) == Some("close")
            && r.get("id") == first.get("id")));
    assert!(requests.borrow().is_empty());
    update(cx, fixture.handle, |shell, _, _| {
        assert!(shell.menu.captured.is_none() && shell.plugins.input.is_none())
    });
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_input_changed_selection_is_never_replaced_with_current_files(
    cx: &mut TestAppContext,
) {
    let (mut fixture, runtime, requests, _) = prompt(cx);
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    assert!(fixture.submit_input(cx, CHECKSUM).is_none());
    assert!(requests.borrow().is_empty());
    update(cx, fixture.handle, |shell, _, _| {
        assert!(shell.menu.captured.is_none())
    });
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_input_plugin_disabled_before_submit_runs_nothing(cx: &mut TestAppContext) {
    let (mut fixture, runtime, requests, _) = prompt(cx);
    update(cx, fixture.handle, |shell, _, _| {
        shell
            .menu
            .plugin_runtime
            .plugins
            .last_mut()
            .unwrap()
            .enabled = false
    });
    assert!(fixture.submit_input(cx, CHECKSUM).is_none());
    assert!(requests.borrow().is_empty());
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_input_rejected_snapshot_and_replaced_paths_run_nothing(
    cx: &mut TestAppContext,
) {
    for replaced in [false, true] {
        let (mut fixture, runtime, requests, first) = prompt(cx);
        let validate = fixture.submit_input(cx, CHECKSUM).unwrap();
        assert_eq!(validate.get("id"), first.get("id"));
        if replaced {
            let other = update(cx, fixture.handle, |shell, _, _| {
                PathBuf::from(&shell.menu.captured.as_ref().unwrap().path).join("photo.png")
            });
            fixture.validated(cx, &validate, &[other]);
        } else {
            fixture.feed(
                cx,
                Json::Obj(vec![
                    ("t".into(), Json::Str("menuaction".into())),
                    ("op".into(), Json::Str("validate".into())),
                    ("id".into(), validate.get("id").unwrap().clone()),
                    ("ok".into(), Json::Bool(false)),
                ]),
            );
        }
        assert!(requests.borrow().is_empty());
        update(cx, fixture.handle, |shell, _, _| {
            assert!(shell.menu.captured.is_none())
        });
        runtime.shutdown();
    }
}

#[gpui::test]
fn plugin_menu_runtime_input_blank_and_oversized_values_stay_in_the_native_field(
    cx: &mut TestAppContext,
) {
    let (mut fixture, runtime, requests, _) = prompt(cx);
    for value in [" ".into(), "x".repeat(129)] {
        assert!(fixture.submit_input(cx, &value).is_none());
        update(cx, fixture.handle, |shell, _, _| {
            assert!(shell.plugins.input.is_some() && shell.menu.captured.is_some())
        });
    }
    fixture.cancel_input(cx);
    assert!(requests.borrow().is_empty());
    runtime.shutdown();
}

#[test]
fn plugin_menu_runtime_input_mismatch_is_an_operation_error_and_the_same_process_can_verify_again()
{
    let root = scratch();
    let _scratch = Scratch(root.clone());
    let (_, runtime) = plugin("checksums", &root);
    let request = |value: &str| {
        Json::Obj(vec![
            ("op".into(), Json::Str("run".into())),
            ("action".into(), Json::Str("sha256".into())),
            (
                "paths".into(),
                Json::Arr(vec![Json::Str(
                    root.join("notes.txt").to_string_lossy().into(),
                )]),
            ),
            (
                "input".into(),
                Json::Obj(vec![("checksum".into(), Json::Str(value.into()))]),
            ),
        ])
    };
    let mismatch = runtime.request(request(&"0".repeat(64))).unwrap().message;
    assert_eq!(mismatch.get("op").and_then(Json::as_str), Some("error"));
    assert_eq!(
        mismatch.get("message").and_then(Json::as_str),
        Some("Checksum does not match.")
    );
    assert!(
        runtime.failure().is_none(),
        "a valid mismatch does not fail the executable"
    );
    assert_eq!(
        runtime
            .request(request(CHECKSUM))
            .unwrap()
            .message
            .get("message")
            .and_then(Json::as_str),
        Some("Checksum matches.")
    );
    let receipt = std::fs::read_to_string(root.join("checksums/requests.jsonl")).unwrap();
    assert_eq!(
        receipt
            .lines()
            .filter(|line| jsondoc::parse(line)
                .unwrap()
                .get("op")
                .and_then(Json::as_str)
                == Some("hello"))
            .count(),
        1
    );
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_input_folder_is_sent_only_with_declared_folder_access(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (mut plugin, runtime) = plugin("checksums", &root);
    plugin.manifest.access.push("folder".into());
    let mut fixture = open(cx, root.clone(), plugin);
    let requests = Rc::new(RefCell::new(Vec::new()));
    let received = requests.clone();
    update(cx, fixture.handle, |shell, _, _| {
        shell.menu.plugin_runtime.dispatch = Some(Box::new(move |_, request| {
            received.borrow_mut().push(request);
            Ok(Json::Null)
        }));
    });
    fixture.menu(cx, 0, vec![0]);
    let first = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    fixture.validated(cx, &first, &[root.join("notes.txt")]);
    let second = fixture.submit_input(cx, CHECKSUM).unwrap();
    fixture.validated(cx, &second, &[root.join("notes.txt")]);
    assert_eq!(requests.borrow().len(), 1);
    assert_eq!(
        requests.borrow()[0].get("folder").and_then(Json::as_str),
        root.to_str()
    );
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_input_backend_death_cancels_the_native_card_and_lease(
    cx: &mut TestAppContext,
) {
    let (fixture, runtime, requests, _) = prompt(cx);
    update(cx, fixture.handle, |shell, window, cx| {
        shell.receive(0, Event::Drained, window, cx)
    });
    cx.run_until_parked();
    update(cx, fixture.handle, |shell, _, _| {
        assert!(shell.plugins.input.is_none() && shell.menu.captured.is_none())
    });
    assert!(requests.borrow().is_empty());
    runtime.shutdown();
}
