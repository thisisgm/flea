//! Live source controls for bounded listings, optical state and identity-safe activation.
use super::*;
use crate::g2::device_rail::{devices::Effect, source::Event};

#[gpui::test]
fn rail_devices_unchanged_poll_does_not_notify_the_window(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    cx.executor().advance_clock(Duration::from_millis(800));
    frame(cx);
    let notifications = Rc::new(std::cell::Cell::new(0));
    let heard = notifications.clone();
    let source_notifications = Rc::new(std::cell::Cell::new(0));
    let source_heard = source_notifications.clone();
    let devices = source(&shell, cx);
    cx.update(|_, cx| {
        cx.observe(&shell, move |_, _| heard.set(heard.get() + 1))
            .detach();
        cx.observe(&devices, move |_, _| {
            source_heard.set(source_heard.get() + 1)
        })
        .detach();
    });
    let before = notifications.get();
    let shown = labels(&shell, cx);
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    assert_eq!(
        service.jobs.borrow().len(),
        4,
        "the periodic lsblk and gio polls still run"
    );
    cx.simulate_frame_request(gpui::RequestFrameOptions::default());
    cx.run_until_parked();
    assert_eq!(
        source_notifications.get(),
        0,
        "polls do not invalidate source readers"
    );
    assert_eq!(
        notifications.get(),
        before,
        "starting a background poll does not change the scene"
    );
    service.answer("lsblk", 1, BLOCKS);
    service.answer("gio", 1, PHONE);
    cx.run_until_parked();
    cx.simulate_frame_request(gpui::RequestFrameOptions::default());
    cx.run_until_parked();
    assert_eq!(labels(&shell, cx), shown);
    assert_eq!(
        source_notifications.get(),
        0,
        "identical answers do not invalidate source readers"
    );
    assert_eq!(
        notifications.get(),
        before,
        "identical discovery answers keep the presented scene"
    );
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    service.answer("lsblk", 2, &BLOCKS.replace("Backup", "Renamed"));
    service.answer("gio", 2, PHONE);
    cx.run_until_parked();
    cx.simulate_frame_request(gpui::RequestFrameOptions::default());
    cx.run_until_parked();
    assert!(labels(&shell, cx).contains(&"Renamed".to_owned()));
    assert!(notifications.get() > before);
    assert!(source_notifications.get() > 0);
}

#[gpui::test]
fn rail_devices_unchanged_mount_poll_still_emits_discovery_state(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let devices = source(&shell, cx);
    let snapshots = Rc::new(RefCell::new(Vec::new()));
    let heard = snapshots.clone();
    cx.update(|_, cx| {
        cx.subscribe(
            &devices,
            move |source, _: &crate::g2::device_rail::source::MountsChanged, cx| {
                let (generation, listing) = source.read(cx).mount_snapshot();
                heard.borrow_mut().push((generation, listing.running));
            },
        )
        .detach();
    });
    let generation = devices.read_with(cx, |source, _| source.mount_snapshot().0);
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    assert_eq!(&*snapshots.borrow(), &[(generation, true)]);
    service.answer("lsblk", 1, BLOCKS);
    service.answer("gio", 1, PHONE);
    cx.run_until_parked();
    assert_eq!(
        &*snapshots.borrow(),
        &[(generation, true), (generation + 1, false)]
    );
}

#[gpui::test]
fn rail_devices_queued_action_still_reaches_the_window_without_row_change(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let notifications = Rc::new(std::cell::Cell::new(0));
    let heard = notifications.clone();
    cx.update(|_, cx| {
        cx.observe(&shell, move |_, _| heard.set(heard.get() + 1))
            .detach();
    });
    let archive = producer(&shell, cx).read_with(cx, |producer, _| {
        producer
            .entries()
            .into_iter()
            .find(|row| row.label == "Archive")
            .unwrap()
    });
    source(&shell, cx).update(cx, |source, cx| {
        source.activate(&archive, 0, cx);
    });
    cx.run_until_parked();
    assert!(
        notifications.get() > 0,
        "the queued action wakes the window"
    );
    frame(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().pane.place().to_owned()
        }),
        "/media/archive"
    );
}

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
fn rail_devices_locked_row_opens_the_unlock_card_and_a_phone_opens_through_gvfs(
    cx: &mut TestAppContext,
) {
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
    // The phone's GVFS flight runs on the network owner's own runner, built on the next frame.
    let network = Rc::new(FakeDevices::default());
    shell.update(cx, |shell, _| {
        let join = shell.rail.as_mut().unwrap();
        join.network = None;
        join.network_runner = network.clone();
    });
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
    producer.update(cx, |producer, cx| {
        producer.side = 0;
        producer.activate(backup, cx)
    });
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        let unlock = shell
            .rail
            .as_ref()
            .unwrap()
            .unlock
            .as_ref()
            .expect("the locked row built the unlock card");
        let card = unlock
            .view
            .read(cx)
            .host
            .card
            .as_ref()
            .expect("the card opened");
        assert!(card.opened, "the locked row opens the unlock card");
        assert_eq!(
            (
                card.device.as_str(),
                card.label.as_str(),
                card.crypto.as_str()
            ),
            ("/dev/sdb1", "Backup", "LUKS"),
            "the card names the row it opened over"
        );
        assert!(
            shell.card_holds_keyboard(cx),
            "the open card holds the keyboard"
        );
    });
    assert_eq!(
        service.jobs.borrow().len(),
        2,
        "unlock never runs gio mount on the locked container"
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(
            !shell.card_holds_keyboard(cx),
            "Escape closes the card and hands the keyboard back"
        );
    });
    producer.update(cx, |producer, cx| {
        producer.side = 0;
        producer.activate(phone, cx)
    });
    frame(cx);
    let started = network
        .jobs
        .borrow()
        .iter()
        .map(|(argv, _, _)| argv.join(" "))
        .collect::<Vec<_>>();
    assert!(
        started.contains(&"gio info -- mtp://pixel/".to_string()),
        "a mounted phone resolves its FUSE folder through the GVFS opener: {started:?}"
    );
    assert!(
        heard.borrow().is_empty(),
        "both rows are owned, so no request is left for an absent subscriber"
    );
}

#[gpui::test]
fn rail_devices_mount_completion_keeps_origin_after_other_pane_activation(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let (backup, photos, archive) = producer(&shell, cx).read_with(cx, |producer, _| {
        let rows = producer.entries();
        let row = |label: &str| rows.iter().find(|row| row.label == label).unwrap().clone();
        (row("Backup"), row("Photos"), row("Archive"))
    });
    let source = source(&shell, cx);
    source.update(cx, |source, cx| {
        source.activate(&backup, 0, cx);
        // A second unmounted device while the first mount runs is refused and never takes the mount owner.
        source.activate(&photos, 1, cx);
        // A mounted one still opens at once, on the pane that asked.
        source.activate(&archive, 1, cx);
        let requests = source.drain_events();
        assert!(matches!(&requests[..], [refused, opened] if refused.side == 1
            && matches!(&refused.event, Event::Device(Effect::Message(note, false)) if note.key == "device.another_mounting")
            && opened.side == 1
            && matches!(&opened.event, Event::Device(Effect::Opened(path)) if path == "/media/archive")));
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
