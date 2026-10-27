//! A Favorites menu removal goes through the locked settings operation writer.
use crate::window::dispatch::pb_places_tests::menu_fixture;
use gpui::TestAppContext;

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
