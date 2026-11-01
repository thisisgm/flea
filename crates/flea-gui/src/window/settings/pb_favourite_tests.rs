//! A Favorites menu removal goes through the locked settings operation writer.
use crate::window::dispatch::pb_places_tests::menu_fixture;
use gpui::TestAppContext;

#[gpui::test]
fn uish_misc_rename_reader_reports_the_backend_loss_state(cx: &mut TestAppContext) {
    let (shell, cx) = menu_fixture(cx, true);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Failed(crate::backend::Failure {
                    key: "backend.closed",
                    detail: "closed reply stream".into(),
                }),
                window,
                cx,
            );
        })
    });
    let state = crate::window::u07_tests::read(&shell, cx, "renameState", &[]);
    let state = flea::jsondoc::parse(&state).unwrap();
    assert_eq!(
        state
            .get("listingState")
            .and_then(flea::jsondoc::Json::as_str),
        Some("error")
    );
    assert_eq!(
        state.get("index").and_then(flea::jsondoc::Json::as_f64),
        Some(-1.)
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_places_legacy_remove_keeps_original_record_and_atomic_operation(cx: &mut TestAppContext) {
    let (shell, cx) = menu_fixture(cx, true);
    let (writes, queued) = std::sync::mpsc::channel();
    let (_updates, updates) = async_channel::unbounded();
    shell.update(cx, |shell, _| {
        shell.settings.worker = Some(crate::window::settings::store::Worker { writes, updates });
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (_, _, operation) = queued
        .try_recv()
        .expect("the chosen row must reach the settings writer");
    let operation = operation.expect("favorite removal must use the locked operation lane");
    assert_eq!(
        operation.get("op").and_then(flea::jsondoc::Json::as_str),
        Some("remove")
    );
    assert_eq!(
        operation.get("expected"),
        Some(&flea::jsondoc::parse(r#"[{"path":"/target","label":"Work"}]"#).unwrap())
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn uish_misc_a_bad_favourite_in_settings_says_why_and_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = menu_fixture(cx, true);
    let before = crate::window::u07_tests::read(&shell, cx, "path", &[]);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.launch.settings.data = flea::jsondoc::parse(
                r#"{"places":{"favourites":[{"label":"Unopenable","path":"relative/unopenable"}]}}"#,
            )
            .unwrap();
            shell.retire_menu(window, cx);
            shell.open_settings(window, cx);
            shell.settings.panel.as_ref().unwrap().update(cx, |view, cx| {
                view.panel.show_section("places");
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let centre = crate::window::u07_tests::read(&shell, cx, "settingsRowCentre", &["favourite:0"]);
    assert!(
        !centre.is_empty(),
        "the painted favorite must have a pointer target"
    );
    crate::window::u07_tests::press(cx, &centre, 1);
    cx.run_until_parked();
    let said = crate::window::u07_tests::read(&shell, cx, "lastMessage", &[]);
    assert!(said.starts_with("Could not open Unopenable · "), "{said}");
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "path", &[]),
        before
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
