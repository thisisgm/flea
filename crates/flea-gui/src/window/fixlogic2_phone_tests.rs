//! A phone mount lands on its DEVICES row at the info exit, as v0.3.12 NetworkMounts.qml infoProcess.onExited re-polls.
use super::*;
use crate::g2::drag_drop::join::rail_drop::rail_kind;
use flea::ui_logic_drag_drop::{rail_destination, RailKind};

const UNMOUNTED: &str = "Volume(0): Pixel 8\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://pixel/\n  can_mount=1\n";

fn relists(service: &FakeDevices) -> usize {
    service
        .jobs
        .borrow()
        .iter()
        .filter(|(argv, _, _)| argv.iter().any(|arg| arg == "-li"))
        .count()
}

fn exit(runner: &FakeDevices, argv: &str, code: i32, stdout: &str) {
    let jobs = runner.jobs.borrow();
    let (_, sender, _) = jobs
        .iter()
        .find(|(started, _, _)| started.join(" ") == argv)
        .unwrap_or_else(|| panic!("the opener started {argv}"));
    sender
        .try_send(ChildEvent::Exited(Outcome {
            code,
            stdout: stdout.into(),
            stderr: String::new(),
        }))
        .unwrap();
}

fn phone_mounted(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<bool> {
    producer(shell, cx).read_with(cx, |producer, _| {
        producer
            .entries()
            .iter()
            .find(|row| row.label == "Pixel 8")
            .and_then(|row| row.mounted)
    })
}

/// Clicks the unmounted phone, lets gio mount succeed, then answers gio info; returns the re-lists before and after the info exit.
fn info_exit<'a>(
    code: i32,
    stdout: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<Shell>,
    Rc<FakeDevices>,
    &'a mut VisualTestContext,
    usize,
    usize,
) {
    let (shell, service, cx) = open("{}", cx);
    service.answer("lsblk", 0, BLOCKS);
    service.answer("gio", 0, UNMOUNTED);
    frame(cx);
    assert_eq!(phone_mounted(&shell, cx), Some(false));
    let network = Rc::new(FakeDevices::default());
    shell.update(cx, |shell, _| {
        let join = shell.rail.as_mut().unwrap();
        join.network = None;
        join.network_runner = network.clone();
    });
    let producer = producer(&shell, cx);
    let phone = producer.read_with(cx, |producer, _| {
        producer
            .entries()
            .iter()
            .position(|row| row.label == "Pixel 8")
            .unwrap()
    });
    let clicked = relists(&service);
    producer.update(cx, |producer, cx| {
        producer.side = 0;
        producer.activate(phone, cx)
    });
    frame(cx);
    exit(&network, "gio mount -- mtp://pixel/", 0, "");
    frame(cx);
    let before = relists(&service);
    assert_eq!(
        before, clicked,
        "the click and the mount exit re-list nothing"
    );
    exit(&network, "gio info -- mtp://pixel/", code, stdout);
    // No clock advance: only the info exit can ask, never the five second poll.
    frame(cx);
    let after = relists(&service);
    (shell, service, cx, before, after)
}

#[gpui::test]
fn fixlogic2_phone_mount_relists_the_rail_at_its_info_exit(cx: &mut TestAppContext) {
    let (shell, service, cx, before, after) =
        info_exit(0, "local path: /run/user/1000/gvfs/mtp:host=pixel\n", cx);
    assert_eq!(
        after,
        before + 1,
        "the info exit re-lists the rail's phone rows once, and the open after it adds none"
    );
    service.answer("gio", 1, PHONE);
    frame(cx);
    assert_eq!(
        phone_mounted(&shell, cx),
        Some(true),
        "the re-list flips the phone row to mounted"
    );
}

#[gpui::test]
fn fixlogic2_failed_info_exit_still_relists_the_rail(cx: &mut TestAppContext) {
    let (_, _, _, before, after) = info_exit(1, "", cx);
    assert_eq!(
        after,
        before + 1,
        "an info exit with no folder opens nothing and still re-lists, as infoProcess.onExited polls first"
    );
}

// Ruling 48: gvfs-afc's iPhone volume in `gio mount -li`, the AFC leg of the iOS 26.6.2 capture in src/ui_logic_device_rail/phones_tests.rs iphone().
const AFC: &str = "Volume(0): Documents on GM’s iPhone\n  Type: GProxyVolume (GProxyVolumeMonitorAfc)\n  ids:\n   uuid: '00008130-001641411883401C'\n  uuid=00008130-001641411883401C\n  activation_root=afc://00008130-001641411883401C:3/\n  can_mount=1\n  can_eject=0\n  should_automount=1\n";

fn devices_tail(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> RailEntry {
    producer(shell, cx).read_with(cx, |producer, _| {
        producer
            .sections
            .last()
            .unwrap()
            .rows
            .last()
            .unwrap()
            .clone()
    })
}

fn phone_row(listing: &str, cx: &mut TestAppContext) -> RailEntry {
    let (shell, service, cx) = open("{}", cx);
    service.answer("lsblk", 0, BLOCKS);
    service.answer("gio", 0, listing);
    frame(cx);
    devices_tail(&shell, cx)
}

#[gpui::test]
fn fixlogic2_afc_volume_reaches_the_same_phone_row_as_mtp(cx: &mut TestAppContext) {
    let mtp = phone_row(PHONE.split("  Mount(0)").next().unwrap(), cx);
    let afc = phone_row(AFC, cx);
    let shape = |row: &RailEntry| (row.group, row.kind, row.glyph, row.mounted);
    assert_eq!(
        shape(&afc),
        shape(&mtp),
        "PhoneMounts AFC rows draw as the MTP row does: a DEVICES phone row, its glyph and its mount state"
    );
    assert_eq!(
        shape(&afc),
        (Group::Device, Kind::Phone, "smartphone", Some(false))
    );
    assert_eq!(afc.label.as_str(), "GM’s iPhone");
    assert_eq!(
        (afc.uri.as_str(), mtp.uri.as_str()),
        ("afc://00008130-001641411883401C/", "mtp://pixel/"),
        "the AFC row opens the phone's root, never the documents volume, the way MTP opens its activation root"
    );
    let drop = |kind| {
        rail_destination(
            kind,
            "/home/gm/Documents",
            true,
            &["file:///home/notes.txt".to_owned()],
            None,
            true,
        )
    };
    // The control: a folder row given the same mounted path takes this drop, so a refusal below is the kind's alone.
    assert_eq!(drop(RailKind::Folder), "/home/gm/Documents");
    for row in [&afc, &mtp] {
        let kind = rail_kind(row.kind);
        assert_eq!(
            kind,
            RailKind::Other,
            "RailDrop maps {} to no drop folder",
            row.uri
        );
        assert!(
            drop(kind).is_empty(),
            "RailDrop refuses a drop on {}",
            row.uri
        );
    }
}
