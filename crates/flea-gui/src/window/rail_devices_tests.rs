//! Device service replies through the mounted rail, its readiness gate and its actual painted rows.
use super::*;
use crate::g2::pl1::{
    process::{Event as ChildEvent, Handle, Job, KillProbe, Outcome},
    source::NetworkSnapshot,
};
use flea::ui_logic_pl4::{
    order::Heading,
    rail::{Group, Kind, RailEntry},
};
use gpui::{TestAppContext, VisualTestContext};
use std::{cell::RefCell, time::Duration};

const BLOCKS: &str = r#"{"blockdevices":[
{"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","size":1000000000,"children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","type":"part","mountpoints":["/"],"fstype":"ext4"}]},
{"name":"sda1","path":"/dev/sda1","type":"part","label":"Archive","mountpoints":["/media/archive"],"fstype":"ext4","rm":true,"size":1000000},
{"name":"sdb1","path":"/dev/sdb1","type":"part","label":"Backup","mountpoints":[null],"fstype":"ext4","size":2000000},
{"name":"sr0","path":"/dev/sr0","type":"rom","model":"DVD","label":"Photos","mountpoints":[null],"fstype":"iso9660","rm":true,"size":3000000}
]}"#;
const PHONE: &str = "Volume(0): Pixel 8\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  activation_root=mtp://pixel/\n  can_mount=1\n  Mount(0): Pixel 8 -> mtp://pixel/\n";
const SETTINGS: &str = r#"{"places":{"favourites":[{"label":"My Work","path":"~/Work"},{"label":"Shared","path":"/srv/shared"}]}}"#;

type RecordedJob = (Vec<String>, async_channel::Sender<ChildEvent>, KillProbe);

#[derive(Default)]
struct FakeDevices {
    jobs: RefCell<Vec<RecordedJob>>,
}

impl Runner for FakeDevices {
    fn start(&self, job: Job) -> Handle {
        let (handle, sender, probe) = Handle::probed();
        self.jobs.borrow_mut().push((job.argv, sender, probe));
        handle
    }
}

impl FakeDevices {
    fn answer(&self, program: &str, occurrence: usize, body: &str) {
        let jobs = self.jobs.borrow();
        let (_, sender, _) = jobs
            .iter()
            .filter(|(argv, _, _)| argv[0] == program)
            .nth(occurrence)
            .expect("the mounted source started this job");
        sender
            .try_send(ChildEvent::Exited(Outcome {
                code: 0,
                stdout: body.into(),
                stderr: String::new(),
            }))
            .unwrap();
    }
}

fn open<'a>(
    state: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, Rc<FakeDevices>, &'a mut VisualTestContext) {
    open_seeded(state, None, cx)
}

// The same window over a launch lsblk answer, as main's startup worker hands it over.
fn open_seeded<'a>(
    state: &str,
    seed: Option<crate::g2::pl1::source::seed::Seed>,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, Rc<FakeDevices>, &'a mut VisualTestContext) {
    let service = Rc::new(FakeDevices::default());
    let runner = service.clone();
    let state = flea::jsondoc::parse(state).unwrap();
    let (backend, receiver) = crate::backend::Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        cx.on_release(move |_, _| drop(receiver)).detach();
        shell.launch.settings.data = state.clone();
        let join = mount_with_source(
            shell.locale.catalog.clone(),
            &state,
            0,
            runner,
            Some("testhost\n".into()),
            seed,
            cx,
        );
        join.places.update(cx, |producer, cx| {
            producer.fixture_home("/fixture/home", "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"", cx)
        });
        shell.slots.rail = Some(AnyView::from(join.host.clone()));
        shell.rail = Some(join);
        shell
    });
    cx.run_until_parked();
    (shell, service, cx)
}

fn producer(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<Producer> {
    shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone())
}

fn source(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Entity<Source> {
    shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().devices.clone())
}

fn frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

// Production readiness: the device answers alone, with no network snapshot injected.
fn ready(service: &FakeDevices, cx: &mut VisualTestContext) {
    service.answer("lsblk", 0, BLOCKS);
    service.answer("gio", 0, PHONE);
    frame(cx);
}

fn labels(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    producer(shell, cx).read_with(cx, |producer, _| {
        producer
            .shown_entries()
            .iter()
            .map(ToString::to_string)
            .collect()
    })
}

#[gpui::test]
fn rail_devices_mounts_exact_shipped_group_and_flat_row_order(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(SETTINGS, cx);
    service.answer("lsblk", 0, BLOCKS);
    service.answer("gio", 0, PHONE);
    source(&shell, cx).update(cx, |source, cx| {
        source.set_network(
            NetworkSnapshot {
                answered: true,
                rows: vec![RailEntry {
                    group: Group::Network,
                    kind: Kind::Share,
                    label: "NAS".into(),
                    uri: "smb://nas/share".into(),
                    glyph: "server",
                    ..RailEntry::default()
                }],
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
            "My Work",
            "Shared",
            "NAS",
            "testhost",
            "Archive",
            "Backup",
            "Photos",
            "Pixel 8"
        ]
    );
    producer(&shell, cx).read_with(cx, |producer, _| {
        assert_eq!(
            producer
                .sections
                .iter()
                .map(|s| s.heading.clone())
                .collect::<Vec<_>>(),
            [
                Heading::Key("places.heading.places"),
                Heading::Key("places.heading.favorites"),
                Heading::Key("settings.places.network"),
                Heading::Key("settings.places.devices")
            ]
        );
        assert_eq!(
            producer
                .provider
                .drawn
                .iter()
                .map(|s| s.heading.as_ref())
                .collect::<Vec<_>>(),
            ["PLACES", "FAVORITES", "NETWORK", "DEVICES"]
        );
        let devices = &producer.sections.last().unwrap().rows;
        assert_eq!(
            devices
                .iter()
                .map(|e| (e.kind, e.glyph, e.mounted))
                .collect::<Vec<_>>(),
            [
                (Kind::Disk, "drive", Some(true)),
                (Kind::Volume, "drive", Some(true)),
                (Kind::Volume, "drive", Some(false)),
                (Kind::Volume, "drive", Some(false)),
                (Kind::Phone, "smartphone", Some(true))
            ]
        );
        let laid = producer.laid.borrow();
        assert!(laid.painted);
        assert_eq!(
            laid.rows
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Home",
                "Downloads",
                "Trash",
                "My Work",
                "Shared",
                "NAS",
                "testhost",
                "Archive",
                "Backup",
                "Photos",
                "Pixel 8"
            ]
        );
        assert!(laid.rows.iter().all(|row| row.rect.height == 29.));
    });
}

#[gpui::test]
fn rail_devices_show_devices_false_removes_rows_and_heading(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(r#"{"places":{"showDevices":false}}"#, cx);
    ready(&service, cx);
    assert_eq!(labels(&shell, cx), ["Home", "Downloads", "Trash"]);
    producer(&shell, cx).read_with(cx, |producer, _| {
        assert_eq!(
            producer
                .provider
                .drawn
                .iter()
                .map(|s| s.heading.as_ref())
                .collect::<Vec<_>>(),
            ["PLACES"]
        )
    });
    let state = flea::jsondoc::parse(r#"{"places":{"showDevices":true}}"#).unwrap();
    shell.update(cx, |shell, cx| shell.rail_state(&state, cx));
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
            "Photos",
            "Pixel 8"
        ]
    );
}

#[gpui::test]
fn rail_devices_periodic_disappearance_is_gone_in_next_frame(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(SETTINGS, cx);
    ready(&service, cx);
    assert!(labels(&shell, cx).iter().any(|label| label == "Backup"));
    cx.executor().advance_clock(Duration::from_millis(5_000));
    cx.run_until_parked();
    let mut blocks = flea::jsondoc::parse(BLOCKS).unwrap();
    if let Json::Obj(members) = &mut blocks {
        if let Some((_, Json::Arr(rows))) =
            members.iter_mut().find(|(key, _)| key == "blockdevices")
        {
            rows.retain(|row| row.get("name").and_then(Json::as_str) != Some("sdb1"));
        }
    }
    service.answer("lsblk", 1, &flea::jsondoc::render_at(&blocks, 0));
    service.answer("gio", 1, "");
    frame(cx);
    assert_eq!(
        labels(&shell, cx),
        [
            "Home",
            "Downloads",
            "Trash",
            "My Work",
            "Shared",
            "testhost",
            "Archive",
            "Photos"
        ]
    );
    producer(&shell, cx).read_with(cx, |producer, _| {
        assert!(!producer
            .laid
            .borrow()
            .rows
            .iter()
            .any(|row| row.label == "Backup" || row.label == "Pixel 8"))
    });
}

#[gpui::test]
fn rail_devices_saved_favourites_keep_labels_paths_order_and_folder_marks(cx: &mut TestAppContext) {
    let (shell, service, cx) = open(SETTINGS, cx);
    ready(&service, cx);
    producer(&shell, cx).read_with(cx, |producer, _| {
        let favourites = &producer.sections[1];
        assert_eq!(favourites.heading, Heading::Key("places.heading.favorites"));
        assert_eq!(
            favourites
                .rows
                .iter()
                .map(|row| (
                    row.label.as_str(),
                    row.path.as_str(),
                    row.glyph,
                    row.favourite_index
                ))
                .collect::<Vec<_>>(),
            [
                ("My Work", "/fixture/home/Work", "folder", Some(0)),
                ("Shared", "/srv/shared", "folder", Some(1))
            ]
        );
        assert_eq!(
            producer.provider.drawn[1]
                .rows
                .iter()
                .map(|row| row.label.as_ref())
                .collect::<Vec<_>>(),
            ["My Work", "Shared"]
        );
    });
}

#[path = "rail_devices_gate_tests.rs"]
mod gates;

#[path = "rail_devices_source_tests.rs"]
mod sources;

#[path = "rail_devices_constructor_tests.rs"]
mod constructor;

#[path = "rail_devices_eject_tests.rs"]
mod eject;

#[path = "rail_devices_release_tests.rs"]
mod release;

#[path = "rail_device_menu_tests.rs"]
mod menus;

#[path = "rail_drive_tests.rs"]
mod drive;

#[path = "fixlogic2_phone_tests.rs"]
mod fixlogic2_phone;
