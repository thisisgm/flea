use super::*;
use gpui::VisualTestContext;

fn draw(fixture: &Fixture, cx: &mut TestAppContext) {
    cx.update_window(fixture.handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn enter_digest(fixture: &mut Fixture, digest: &str, cx: &mut TestAppContext) {
    fixture.menu(cx, 0, vec![0]);
    wait(cx, |cx| {
        fixture
            .entries(cx)
            .iter()
            .any(|r| r.action == "example.checksums:sha256")
    });
    let validate = fixture.choose(cx, "example.checksums:sha256", "").unwrap();
    let paths = [fixture._scratch.0.join("notes.txt")];
    fixture.validated(cx, &validate, &paths);
    draw(fixture, cx);
    update(cx, fixture.handle, |shell, _, _| {
        let entry = &shell.rename.ime.entry;
        assert!(entry.text.is_empty() || entry.selected == (0..entry.text.len()));
    });
    VisualTestContext::from_window(fixture.handle.into(), cx).simulate_input(digest);
    draw(fixture, cx);
    update(cx, fixture.handle, |shell, _, _| {
        assert_eq!(shell.plugins.input.as_ref().unwrap().value, digest);
        assert_eq!(shell.rename.ime.entry.text, digest);
    });
    VisualTestContext::from_window(fixture.handle.into(), cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    let request = fixture
        .drain()
        .into_iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
        .unwrap();
    fixture.validated(cx, &request, &paths);
}

#[gpui::test]
fn plugin_action_recovery_mismatch_then_match_in_one_owner(cx: &mut TestAppContext) {
    let (mut fixture, folder) = fixture(cx);
    let responses = Rc::new(RefCell::new(Vec::<crate::g2::plugins::Response>::new()));
    let captured = responses.clone();
    let _subscription = update(cx, fixture.handle, |shell, window, cx| {
        let host = shell.plugins.host.clone().unwrap();
        cx.subscribe_in(
            &host,
            window,
            move |_, _, response: &crate::g2::plugins::Response, _, _| {
                if response.plugin == "example.checksums"
                    && response.request.get("op").and_then(Json::as_str) == Some("run")
                {
                    captured.borrow_mut().push(response.clone());
                }
            },
        )
    });
    enter_digest(&mut fixture, &"0".repeat(64), cx);
    wait(cx, |cx| {
        notice(&fixture, cx).error && notice(&fixture, cx).pending == 0
    });
    release(&fixture, &folder);
    enter_digest(&mut fixture, CHECKSUM, cx);
    wait(cx, |cx| {
        responses
            .borrow()
            .iter()
            .filter(|r| {
                matches!(
                    r.message.get("op").and_then(Json::as_str),
                    Some("error" | "done")
                )
            })
            .count()
            == 2
            && notice(&fixture, cx).pending == 0
    });
    let terminal: Vec<_> = responses
        .borrow()
        .iter()
        .filter(|r| {
            matches!(
                r.message.get("op").and_then(Json::as_str),
                Some("error" | "done")
            )
        })
        .cloned()
        .collect();
    assert_eq!(terminal.len(), 2);
    assert_ne!(terminal[0].id, terminal[1].id);
    assert!(!terminal[0].ok && terminal[1].ok);
    assert_eq!(
        terminal[0]
            .request
            .get("input")
            .unwrap()
            .get("checksum")
            .and_then(Json::as_str),
        Some("0".repeat(64).as_str())
    );
    assert_eq!(
        terminal[1]
            .request
            .get("input")
            .unwrap()
            .get("checksum")
            .and_then(Json::as_str),
        Some(CHECKSUM)
    );
    assert_eq!(
        receipt(&folder, "hello").len(),
        1,
        "the owner process did not restart"
    );
    let result = notice(&fixture, cx);
    println!(
        "RECOVERY ids: {}, {}; second input: {}; terminal: {:?}; shown: {}",
        terminal[0].id, terminal[1].id, CHECKSUM, terminal[1].message, result.shown
    );
    assert_eq!(
        result.shown, "Checksum matches.",
        "the old verdict must not masquerade as this run's result"
    );
    assert!(!result.error);
    stop(cx, &fixture);
}

fn overlapping_verdicts(cx: &mut TestAppContext, older_last: bool) {
    let (mut fixture, _folder) = fixture(cx);
    fixture.menu(cx, 0, vec![0]);
    update(cx, fixture.handle, |shell, window, cx| {
        shell.retire_menu(window, cx);
        let request = jsondoc::parse(r#"{"op":"run","action":"sha256"}"#).unwrap();
        for id in [101, 102] {
            shell.menu.plugin_runtime.pending.insert(id, 0);
            shell.plugin_action_sent(
                crate::g2::plugins::menu::verdict::owner(0, "example.checksums", &request),
                id,
                cx,
            );
        }
        let ids = if older_last { [102, 101] } else { [101, 102] };
        let host = shell.plugins.host.clone().unwrap();
        for id in ids {
            host.update(cx, |_, cx| {
                cx.emit(crate::g2::plugins::Response {
                    id,
                    plugin: "example.checksums".into(),
                    request: request.clone(),
                    message: if id == 102 {
                        jsondoc::parse(r#"{"op":"done","message":"Checksum matches."}"#).unwrap()
                    } else {
                        jsondoc::parse(r#"{"op":"error"}"#).unwrap()
                    },
                    ok: id == 102,
                    error: if id == 101 {
                        "Checksum does not match.".into()
                    } else {
                        String::new()
                    },
                });
            });
        }
    });
    wait(cx, |cx| notice(&fixture, cx).pending == 0);
    let result = notice(&fixture, cx);
    assert_eq!(result.shown, "Checksum matches.");
    assert!(!result.error);
    assert!(result.queued.is_empty());
    stop(cx, &fixture);
}

#[gpui::test]
fn plugin_action_recovery_older_failure_before_latest_success(cx: &mut TestAppContext) {
    overlapping_verdicts(cx, false);
}

#[gpui::test]
fn plugin_action_recovery_older_failure_after_latest_success(cx: &mut TestAppContext) {
    overlapping_verdicts(cx, true);
}
