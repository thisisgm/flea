//! Live source controls for bounded listings, optical state and identity-safe activation.
use super::*;
use crate::g2::pl1::{devices::Effect, source::Event};

#[gpui::test]
fn rail_devices_listing_timeout_keeps_last_complete_rows(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&shell, &service, cx);
    let before = labels(&shell, cx);
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(10_000));
    cx.run_until_parked();
    assert!(service.jobs.borrow()[2..]
        .iter()
        .all(|(_, _, probe)| probe.killed()));
    service.answer("lsblk", 1, "partial");
    service.answer("gio", 1, "partial");
    frame(cx);
    assert_eq!(
        labels(&shell, cx),
        before,
        "a killed listing never replaces the rail"
    );
}

#[gpui::test]
fn rail_devices_empty_optical_drive_is_hidden(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    service.answer("lsblk", 0, &BLOCKS.replace("\"fstype\":\"iso9660\",", ""));
    service.answer("gio", 0, PHONE);
    source(&shell, cx).update(cx, |source, cx| {
        source.set_network(
            NetworkSnapshot {
                answered: true,
                rows: Vec::new(),
            },
            cx,
        )
    });
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
            "Pixel 8"
        ]
    );
}

#[gpui::test]
fn rail_devices_unlock_row_retains_lock_glyph_and_typed_request(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    service.answer(
        "lsblk",
        0,
        &BLOCKS.replace(
            "\"fstype\":\"ext4\",\"size\":2000000",
            "\"fstype\":\"crypto_LUKS\",\"size\":2000000",
        ),
    );
    service.answer("gio", 0, PHONE);
    source(&shell, cx).update(cx, |source, cx| {
        source.set_network(
            NetworkSnapshot {
                answered: true,
                rows: Vec::new(),
            },
            cx,
        )
    });
    frame(cx);
    let row = producer(&shell, cx).read_with(cx, |producer, _| {
        producer
            .entries()
            .into_iter()
            .find(|row| row.label == "Backup")
            .unwrap()
    });
    assert_eq!(row.glyph, "lock");
    let source = source(&shell, cx);
    source.update(cx, |source, cx| {
        assert!(source.activate(&row, 1, cx));
        let requests = source.drain_events();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].side, 1);
        assert!(matches!(&requests[0].event, Event::Device(Effect::UnlockRequested { device, label, size: 2_000_000, fstype }) if device == "/dev/sdb1" && label == "Backup" && fstype == "crypto_LUKS"));
    });
    assert_eq!(
        service.jobs.borrow().len(),
        2,
        "unlock never runs gio mount on the locked container"
    );
}

#[gpui::test]
fn rail_devices_mount_completion_keeps_origin_after_other_pane_activation(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&shell, &service, cx);
    let (backup, archive) = producer(&shell, cx).read_with(cx, |producer, _| {
        let rows = producer.entries();
        (
            rows.iter()
                .find(|row| row.label == "Backup")
                .unwrap()
                .clone(),
            rows.iter()
                .find(|row| row.label == "Archive")
                .unwrap()
                .clone(),
        )
    });
    let source = source(&shell, cx);
    source.update(cx, |source, cx| {
        source.activate(&backup, 0, cx);
        source.activate(&archive, 1, cx);
        let requests = source.drain_events();
        assert!(matches!(&requests[0].event, Event::Device(Effect::Opened(path)) if path == "/media/archive"));
        assert_eq!(requests[0].side, 1);
    });
    assert_eq!(
        service.jobs.borrow().last().unwrap().0,
        ["gio", "mount", "-d", "/dev/sdb1"]
    );
    service.answer("gio", 1, "");
    cx.run_until_parked();
    service.answer(
        "lsblk",
        1,
        &BLOCKS.replace(
            "\"label\":\"Backup\",\"mountpoints\":[null]",
            "\"label\":\"Backup\",\"mountpoints\":[\"/media/backup\"]",
        ),
    );
    cx.run_until_parked();
    frame(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.place(),
            "/media/backup"
        );
        assert!(
            shell.panes[1].is_none(),
            "the other pane was never the mount owner"
        );
    });
}

#[gpui::test]
fn rail_devices_vanished_identity_never_mounts_replacement_row(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&shell, &service, cx);
    let stale = producer(&shell, cx).read_with(cx, |producer, _| {
        producer
            .entries()
            .into_iter()
            .find(|row| row.label == "Backup")
            .unwrap()
    });
    let source = source(&shell, cx);
    source.update(cx, |source, cx| {
        source
            .devices
            .entries
            .retain(|row| row.device != "/dev/sdb1");
        source.activate(&stale, 0, cx);
        assert!(source.drain_events().is_empty());
    });
    assert_eq!(
        service.jobs.borrow().len(),
        2,
        "no stale index can start a mount"
    );
}
