//! Issue 232 drive fixtures, with no device operations and every mount point under a marked mktemp root.
use super::*;
use crate::g2::pl1::{
    devices::{Devices, Effect, Proc},
    process::Outcome,
};
use flea::ui_logic_pl1::{devices::parse_devices, menu::RailAction, mounts::rail_key};
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-bfx-drives.XXXXXX"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        std::fs::write(root.join(".flea-bfx-drives-root"), "232 drive fixtures\n").unwrap();
        std::fs::create_dir(root.join("volume")).unwrap();
        Self(root)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().into()
    }

    fn listing(&self, filesystem: &str, mounted: bool) -> String {
        let point = if mounted {
            format!("\"{}\"", self.path("volume"))
        } else {
            "null".into()
        };
        format!(
            r#"{{"blockdevices":[{{"name":"sdb","path":"{}","type":"disk","tran":"sata","rm":false,"children":[{{"name":"sdb2","path":"{}","type":"part","label":"Archive","fstype":"{filesystem}","mountpoints":[{point}]}}]}}]}}"#,
            self.path("disk"),
            self.path("partition")
        )
    }

    fn devices(&self, mounted: bool) -> Devices {
        let mut devices = Devices::new("fixture");
        devices.poll(0);
        devices.list_exited(&self.outcome(mounted));
        devices
    }

    fn outcome(&self, mounted: bool) -> Outcome {
        Outcome {
            stdout: self.listing("hfsplus", mounted),
            ..Outcome::default()
        }
    }

    /// A USB drive holding a mounted stick and an unmounted sibling, beside a fixed drive.
    fn two_drives(&self, removable: bool) -> Outcome {
        Outcome {
            stdout: format!(
                r#"{{"blockdevices":[{{"name":"sdb","path":"/dev/sdb","type":"disk","tran":"usb","rm":{removable},"children":[{{"name":"sdb1","path":"/dev/sdb1","type":"part","rm":{removable},"label":"Stick","fstype":"hfsplus","mountpoints":["{}"]}},{{"name":"sdb2","path":"/dev/sdb2","type":"part","rm":{removable},"label":"Sibling","fstype":"ext4","mountpoints":[null]}}]}},{{"name":"sdc","path":"/dev/sdc","type":"disk","tran":"sata","rm":false,"children":[{{"name":"sdc1","path":"/dev/sdc1","type":"part","label":"Other","fstype":"ext4","mountpoints":["{}"]}}]}}]}}"#,
                self.path("volume"),
                self.path("other")
            ),
            ..Outcome::default()
        }
    }
}

fn says(effects: &[Effect], key: &str) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::Message(note, false) if note.key == key))
}

fn entry_at(devices: &Devices, label: &str) -> usize {
    devices
        .entries
        .iter()
        .position(|entry| entry.label == label)
        .unwrap_or_else(|| panic!("the fixture lists {label}"))
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.join(".flea-bfx-drives-root").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn drives_232_1_usable_internal_ntfs_is_visible_with_unmounted_off() {
    let fixture = Fixture::new();
    for mounted in [false, true] {
        for show_unmounted in [false, true] {
            let rows = parse_devices(&fixture.listing("ntfs", mounted), show_unmounted);
            assert_eq!(
                rows.len(),
                1,
                "usable ntfs stays visible, mounted={mounted}, show_unmounted={show_unmounted}"
            );
            assert_eq!(rows[0].mounted, mounted);
            assert_eq!(rows[0].device, fixture.path("partition"));
            assert_eq!(
                rows[0].path,
                if mounted {
                    fixture.path("volume")
                } else {
                    String::new()
                }
            );
        }
    }
}

#[test]
fn drives_232_1_ntfs_visibility_preserves_hidden_and_other_unmounted_filters() {
    let fixture = Fixture::new();
    let listing = fixture.listing("ntfs", false);
    let part = r#""type":"part""#;
    let reserved = listing.replace("Archive", "System Reserved");
    let firmware = listing.replace(
        part,
        r#""type":"part","parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b""#,
    );
    let recovery = listing.replace(r#""label":"Archive","#, "").replace(
        part,
        r#""type":"part","parttype":"de94bba4-06d1-4d40-a16a-bfd50179d6ac""#,
    );
    let no_filesystem = listing.replace(r#""fstype":"ntfs""#, r#""fstype":"""#);
    let loop_device = listing.replace("sdb", "loop0");
    for show_unmounted in [false, true] {
        for hidden in [
            &reserved,
            &firmware,
            &recovery,
            &no_filesystem,
            &loop_device,
        ] {
            assert!(
                parse_devices(hidden, show_unmounted).is_empty(),
                "hidden rows stay hidden: {hidden}"
            );
        }
    }
    assert!(parse_devices(&fixture.listing("ext4", false), false).is_empty());
    assert_eq!(
        parse_devices(&fixture.listing("ext4", false), true).len(),
        1
    );
}

#[gpui::test]
fn drives_232_5_unmount_releases_the_other_pane_on_hfsplus(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (shell, service, cx) = shell_with_listing(
        &fixture.path("outside"),
        &fixture.listing("hfsplus", true),
        cx,
    );
    let (backend, mut right_lines) = crate::backend::Backend::test_lines();
    LINES.with(|lines| {
        let mut slot = lines.borrow_mut();
        let mut left = slot.take();
        *slot = Some(Box::new(move || {
            right_lines()
                .map(|line| format!("right {line}"))
                .or_else(|| left.as_mut().and_then(|left| left()))
        }));
    });
    shell.update(cx, |shell, _| {
        let mut right =
            crate::window::pane::PaneHost::new(fixture.path("volume"), None, 620., &shell.locale);
        right.backend = Some(backend);
        shell.panes[1] = Some(right);
    });
    drain_lines();
    LOG.with(|log| log.borrow_mut().clear());
    let source = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().devices.clone());
    source.update(cx, |source, cx| {
        let key = rail_key(&source.devices.entries[0]);
        source.release_by(RailAction::UnmountVolume, &key, 0, cx);
    });
    frame(cx);
    let log = LOG.with(|log| log.borrow().clone());
    let at = |wanted: &str| log.iter().position(|line| line == wanted);
    let unmount = at(&format!("job gio mount -u {}", fixture.path("volume")));
    let thumbs = at(r#"backend right {"c":"thumbcancel","rows":[]}"#);
    let sizes = at(r#"backend right {"c":"dirsizecancel"}"#);
    assert!(
        unmount.is_some() && thumbs.is_some() && sizes.is_some(),
        "the other pane's quiesce lines and the unmount job are all sent: {log:?}"
    );
    assert!(
        thumbs < unmount && sizes < unmount,
        "the other pane stops its volume readers before the unmount child starts: {log:?}"
    );
    assert_ne!(
        shell.read_with(cx, |shell, _| shell.panes[1]
            .as_ref()
            .unwrap()
            .pane
            .place()
            .to_owned()),
        fixture.path("volume")
    );
    assert!(jobs(&service).contains(&format!("gio mount -u {}", fixture.path("volume"))));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[test]
fn drives_232_6_activation_cannot_remount_a_releasing_hfsplus_volume() {
    let fixture = Fixture::new();
    for eject in [false, true] {
        let mut devices = fixture.devices(true);
        if eject {
            devices.eject(0, 1);
        } else {
            devices.unmount(0);
        }
        for mounted in [true, false] {
            devices.list_exited(&fixture.outcome(mounted));
            let effects = devices.activate(0);
            assert!(!effects.iter().any(|effect| matches!(effect, Effect::Start(Proc::Mount, _) | Effect::Opened(_))), "a release must neither reopen nor remount its volume, eject={eject}, mounted={mounted}: {effects:?}");
            let key = if eject {
                "device.still_ejecting"
            } else {
                "device.still_unmounting"
            };
            assert!(says(&effects, key));
        }
    }
}

#[test]
fn drives_232_6_a_release_holds_only_the_drive_it_takes() {
    let fixture = Fixture::new();
    for (removable, eject) in [(true, false), (true, true), (false, true)] {
        let mut devices = Devices::new("fixture");
        devices.poll(0);
        devices.list_exited(&fixture.two_drives(removable));
        let stick = entry_at(&devices, "Stick");
        let released = if eject {
            devices.eject(stick, 1)
        } else {
            devices.unmount(stick)
        };
        assert!(
            released
                .iter()
                .any(|effect| matches!(effect, Effect::Quiesce(_))),
            "the release starts, removable={removable}, eject={eject}: {released:?}"
        );
        let other = devices.activate(entry_at(&devices, "Other"));
        assert!(
            other.iter().any(|effect| matches!(effect, Effect::Opened(path) if *path == fixture.path("other"))),
            "a volume on another drive still opens, removable={removable}, eject={eject}: {other:?}"
        );
        let sibling = devices.activate(entry_at(&devices, "Sibling"));
        let mounts = sibling
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Mount, _)));
        assert_eq!(mounts, !eject, "an unmount holds its own volume and an eject its whole drive, removable={removable}: {sibling:?}");
        assert_eq!(says(&sibling, "device.still_ejecting"), eject);
    }
}

#[gpui::test]
fn drives_232_5_unmount_closes_the_retained_preview_before_release(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (shell, _service, cx) = shell_with_listing(
        &fixture.path("volume"),
        &fixture.listing("hfsplus", true),
        cx,
    );
    let preview = shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        let port = host.preview_port.clone();
        let face = host.list.borrow().typeface.clone();
        let catalog = shell.locale.catalog.clone();
        let preview = cx.new(|cx| {
            crate::window::selection_preview::SelectionPreview::new(port, catalog, face, cx)
        });
        preview.update(cx, |preview, _| {
            preview.visible = true;
            preview.drawn = Some(retained_frame(&fixture));
        });
        host.preview = Some(preview.clone());
        preview
    });
    // Read before any frame: a later render may hide the preview for its own reasons.
    let (visible, drawn) = shell.update_in(cx, |shell, window, cx| {
        shell.quiesce_volume(0, &fixture.path("volume"), window, cx);
        preview.read_with(cx, |preview, _| (preview.visible, preview.drawn.is_some()))
    });
    assert!(!visible && !drawn, "a navigation keeps the drawn preview until its replacement lands; release must close it immediately, visible={visible}, drawn={drawn}");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[test]
fn drives_232_6_unmount_waits_for_the_existing_mount_child_to_exit() {
    let fixture = Fixture::new();
    let mut devices = fixture.devices(false);
    devices.activate(0);
    devices.list_exited(&fixture.outcome(true));
    let effects = devices.unmount(0);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Unmount, _))),
        "a late mount child must not race a manual unmount: {effects:?}"
    );
    assert!(says(&effects, "device.another_mounting"), "{effects:?}");
    devices.mount_exited(&Outcome::code(0), 2);
    let control = devices.eject(0, 3);
    assert!(
        control
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Eject, _))),
        "with the mount child gone this fixture ejects, so the refusals above are the mount guard: {control:?}"
    );
}

#[test]
fn drives_232_6_eject_and_unmount_both_exclude_mount_flights() {
    let fixture = Fixture::new();
    let mut devices = fixture.devices(false);
    devices.activate(0);
    devices.list_exited(&fixture.outcome(true));
    let effects = devices.eject(0, 1);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Eject, _))),
        "an unfinished mount excludes eject too: {effects:?}"
    );
    assert!(says(&effects, "device.another_mounting"), "{effects:?}");
    devices.mount_exited(&Outcome::code(0), 2);
    assert!(devices
        .unmount(0)
        .iter()
        .any(|effect| matches!(effect, Effect::Start(Proc::Unmount, _))));
    devices.unmount_exited(&Outcome::code(0), 3);
    for now in [4, 5_004, 10_004] {
        let effects = devices.list_exited(&fixture.outcome(false));
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Mount, _) | Effect::Opened(_))));
        assert!(!devices
            .tick(now)
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Mount, _))));
    }
    assert!(
        devices
            .activate(0)
            .iter()
            .any(|effect| matches!(effect, Effect::Start(Proc::Mount, _))),
        "a fresh explicit activation after completion may mount again"
    );
}

fn retained_frame(fixture: &Fixture) -> crate::window::selection_preview::Frame {
    let row = flea::protocol::RowFacts {
        name: "notes.txt".into(),
        ..Default::default()
    };
    crate::window::selection_preview::Frame {
        subject: crate::window::selection_preview::Subject {
            directory: fixture.path("volume"),
            listing: 1,
            index: 0,
            row: row.clone(),
            selected: vec![row],
            selected_count: 1,
        },
        generation: 1,
        kind: crate::window::selection_preview::Kind::Text,
        mounted: crate::window::selection_preview::Mounted::Basic,
        meta: None,
        facts: None,
        thumb_answered: true,
        thumb_observation: None,
        source: None,
        image: None,
        animation: None,
        animation_source: None,
        quality: Default::default(),
        trace: None,
        failed: false,
        manual: false,
        remote: false,
        _watch: None,
    }
}
