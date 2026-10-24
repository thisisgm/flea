//! Readiness, settings and lazy lifecycle controls for the same mounted fake service.
use super::*;

#[gpui::test]
fn rail_devices_waits_for_both_sources_then_obeys_shipped_deadline(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    service.answer("lsblk", 0, BLOCKS);
    frame(cx);
    assert_eq!(labels(&shell, cx), ["Home", "Downloads", "Trash"]);
    cx.executor().advance_clock(Duration::from_millis(799));
    frame(cx);
    assert_eq!(labels(&shell, cx), ["Home", "Downloads", "Trash"]);
    cx.executor().advance_clock(Duration::from_millis(1));
    frame(cx);
    assert_eq!(
        labels(&shell, cx),
        [
            "Home",
            "Downloads",
            "Trash",
            "testhost",
            "Archive",
            "Backup",
            "Photos"
        ]
    );
    service.answer("gio", 0, PHONE);
    frame(cx);
    assert_eq!(labels(&shell, cx).last().unwrap(), "Pixel 8");
}

#[gpui::test]
fn rail_devices_unmounted_setting_keeps_optical_and_removable_rows(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(r#"{"places":{"showUnmounted":false}}"#, cx);
    ready(&service, cx);
    assert_eq!(
        labels(&shell, cx),
        [
            "Home",
            "Downloads",
            "Trash",
            "testhost",
            "Archive",
            "Photos",
            "Pixel 8"
        ]
    );
}

#[gpui::test]
fn rail_devices_hidden_host_starts_no_jobs_and_withdrawal_parks_polling(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(r#"{"places":{"rail":"hidden"}}"#, cx);
    frame(cx);
    assert!(service.jobs.borrow().is_empty());
    let state = flea::jsondoc::parse(r#"{"places":{"rail":"shown"}}"#).unwrap();
    shell.update(cx, |shell, cx| shell.rail_state(&state, cx));
    frame(cx);
    assert_eq!(service.jobs.borrow().len(), 2);
    ready(&service, cx);
    let hidden = flea::jsondoc::parse(r#"{"places":{"rail":"hidden"}}"#).unwrap();
    shell.update(cx, |shell, cx| shell.rail_state(&hidden, cx));
    frame(cx);
    cx.executor().advance_clock(Duration::from_millis(20_000));
    frame(cx);
    assert_eq!(
        service.jobs.borrow().len(),
        2,
        "hidden rail does not enumerate"
    );
}
