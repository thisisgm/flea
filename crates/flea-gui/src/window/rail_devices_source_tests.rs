//! Live source controls for bounded listings, optical state and identity-safe activation.
use super::*;
use crate::g2::pl1::{devices::Effect, source::Event};

#[gpui::test]
fn rail_devices_listing_timeout_keeps_last_complete_rows(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let before = labels(&shell, cx);
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(10_000));
    cx.run_until_parked();
    assert!(service.jobs.borrow()[2..]
        .iter()
        .all(|(_, _, probe)| probe.killed()));
    // Complete, valid answers that differ from the drawn rows: only the timed-out verdict keeps them out.
    service.answer(
        "lsblk",
        1,
        &BLOCKS.replace("\"label\":\"Backup\"", "\"label\":\"Late\""),
    );
    service.answer("gio", 1, &PHONE.replace("Pixel 8", "Late Phone"));
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
fn rail_devices_locked_and_phone_rows_reach_the_rail_request_owner(cx: &mut TestAppContext) {
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
    frame(cx);
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    let producer = producer(&shell, cx);
    let index = |label: &str, cx: &mut VisualTestContext| {
        producer.read_with(cx, |producer, _| {
            producer
                .entries()
                .iter()
                .position(|row| row.label == label)
                .unwrap()
        })
    };
    let backup = index("Backup", cx);
    let phone = index("Pixel 8", cx);
    assert_eq!(
        producer.read_with(cx, |producer, _| producer.entries()[backup].glyph),
        "lock"
    );
    for (row, side) in [(backup, 0), (phone, 1)] {
        producer.update(cx, |producer, cx| {
            producer.side = side;
            producer.activate(row, cx)
        });
        frame(cx);
    }
    let heard = heard.borrow();
    let reached = heard
        .iter()
        .map(|request| match &request.event {
            crate::g2::pl4::producer::Event::Activate(entry) => (
                request.side,
                entry.label.as_str(),
                entry.device.as_str(),
                entry.uri.as_str(),
            ),
            _ => panic!("only the two activations reach the owner"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reached,
        [
            (0, "Backup", "/dev/sdb1", ""),
            (1, "Pixel 8", "", "mtp://pixel/")
        ],
        "a locked volume and a phone keep their request instead of vanishing"
    );
    assert_eq!(
        service.jobs.borrow().len(),
        2,
        "unlock never runs gio mount on the locked container"
    );
}

#[gpui::test]
fn rail_devices_mount_completion_keeps_origin_after_other_pane_activation(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let (backup, photos) = producer(&shell, cx).read_with(cx, |producer, _| {
        let rows = producer.entries();
        (
            rows.iter()
                .find(|row| row.label == "Backup")
                .unwrap()
                .clone(),
            rows.iter()
                .find(|row| row.label == "Photos")
                .unwrap()
                .clone(),
        )
    });
    let source = source(&shell, cx);
    source.update(cx, |source, cx| {
        source.activate(&backup, 0, cx);
        // A second unmounted device while the first mount runs is refused and never takes the mount owner.
        source.activate(&photos, 1, cx);
        let requests = source.drain_events();
        assert!(matches!(&requests[..], [request] if request.side == 1
            && matches!(&request.event, Event::Device(Effect::Message(note, false)) if note.key == "device.another_mounting")));
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
    ready(&service, cx);
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
