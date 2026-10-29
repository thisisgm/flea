//! #205: a pressed drive opens only on its own mount, as v0.3.12 PickerPlaces.qml deviceOpened matches device and path, and Back returns to Recent.
use super::tests::fixture::{fixture, Fixture};
use crate::g2::pl1::process::{Event as ChildEvent, Handle, Job, KillProbe, Outcome, Runner};
use crate::g2::pl1::source::fixture::Services;
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

// Sample input: lsblk --json with two removable volumes, Alpha on /dev/sdb1 and Beta on /dev/sdc1, each mounted when named.
fn blocks(mounted: &[&str]) -> String {
    let volume = |name: &str, label: &str| {
        let point = if mounted.contains(&label) {
            format!(r#""/run/media/test/{label}""#)
        } else {
            "null".into()
        };
        format!(
            r#"{{"name":"{name}","path":"/dev/{name}","type":"part","label":"{label}","mountpoints":[{point}],"fstype":"ext4","rm":true,"size":1000000}}"#
        )
    };
    format!(
        r#"{{"blockdevices":[{},{}]}}"#,
        volume("sdb1", "Alpha"),
        volume("sdc1", "Beta")
    )
}

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
    // Answers the oldest unanswered job whose argv starts with `head`.
    fn answer(&self, head: &[&str], stdout: &str) {
        let mut jobs = self.jobs.borrow_mut();
        let index = jobs
            .iter()
            .position(|(argv, _, _)| {
                argv.iter()
                    .map(String::as_str)
                    .take(head.len())
                    .eq(head.iter().copied())
            })
            .unwrap_or_else(|| panic!("no {head:?} job was started"));
        let (_, sender, _) = jobs.remove(index);
        sender
            .try_send(ChildEvent::Exited(Outcome {
                code: 0,
                stdout: stdout.into(),
                stderr: String::new(),
            }))
            .unwrap();
    }
}

fn open(cx: &mut TestAppContext) -> (Fixture<'_>, Rc<FakeDevices>) {
    let service = Rc::new(FakeDevices::default());
    let runner = service.clone();
    cx.update(|cx| {
        cx.set_global(Services {
            runner,
            hostname: "testhost".into(),
        })
    });
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.update(|picker, cx| picker.start_places(&flea::jsondoc::Json::Null, cx));
    f.cx.run_until_parked();
    service.answer(&["lsblk"], &blocks(&[]));
    service.answer(&["gio", "mount", "-li"], "");
    f.cx.run_until_parked();
    (f, service)
}

fn press(f: &mut Fixture, label: &str) {
    f.update(|picker, cx| {
        let index = picker
            .rail
            .labels
            .iter()
            .position(|shown| shown.as_ref() == label)
            .unwrap_or_else(|| panic!("{label} is on the rail: {:?}", picker.rail.labels));
        picker.rail_choose(index, cx);
    });
    f.cx.run_until_parked();
}

// Alpha's mount ends and the next listing shows it mounted, which raises its Opened.
fn alpha_mounts(f: &mut Fixture, service: &FakeDevices) {
    service.answer(&["gio", "mount", "-d", "/dev/sdb1"], "");
    f.cx.run_until_parked();
    service.answer(&["lsblk"], &blocks(&["Alpha"]));
    f.cx.run_until_parked();
}

#[gpui::test]
fn a_pressed_drive_mounts_and_opens_its_own_folder(cx: &mut TestAppContext) {
    let (mut f, service) = open(cx);
    press(&mut f, "Alpha");
    alpha_mounts(&mut f, &service);
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/run/media/test/Alpha"
    );
}

#[gpui::test]
fn another_drives_mount_is_not_the_pressed_drives_answer(cx: &mut TestAppContext) {
    let (mut f, service) = open(cx);
    press(&mut f, "Alpha");
    // Beta is refused while Alpha mounts, and it is now the press the dialog waits on.
    press(&mut f, "Beta");
    assert_eq!(
        f.read(|picker| picker.rail.awaiting.clone()),
        "/dev/sdc1",
        "the dialog waits on Beta, so only the device match can turn Alpha's mount away"
    );
    alpha_mounts(&mut f, &service);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
}

#[gpui::test]
fn a_folder_opened_meanwhile_ends_the_drive_wait(cx: &mut TestAppContext) {
    let (mut f, service) = open(cx);
    press(&mut f, "Alpha");
    f.update(|picker, cx| picker.open_folder("/srv", cx));
    alpha_mounts(&mut f, &service);
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/srv");
}

#[gpui::test]
fn back_from_a_folder_opened_out_of_recent_lists_recent_again(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    let history = f.root.0.join("recently-used.xbel");
    f.update(|picker, cx| {
        picker.rail.history = history.to_string_lossy().into_owned();
        picker.rail_choose(0, cx);
        picker.open_folder("/srv/docs", cx);
    });
    f.cx.run_until_parked();
    let sent = f.listing.sent().len();
    f.update(|picker, cx| picker.go_back(cx));
    f.cx.run_until_parked();
    assert!(f.listing.sent().len() > sent, "Back asks for Recent again");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        flea::ui_logic_pk1::RECENT
    );
    assert!(f.listing.sent().last().unwrap().contains("listpaths"));
}
