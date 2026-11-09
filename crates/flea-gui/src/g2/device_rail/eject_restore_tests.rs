//! A refused release hands back the panes it moved only while their volume is still mounted, and a retry's verdict withdraws the refusal.
use super::devices::{Devices, Effect, Moved, Proc};
use super::process::Outcome;

const STICK: &str = r#"{"blockdevices":[{"name":"sda1","path":"/dev/sda1","type":"part","label":"Stick","mountpoints":["/media/stick"],"fstype":"exfat","rm":true}]}"#;
const STICK_GONE: &str = r#"{"blockdevices":[{"name":"sda1","path":"/dev/sda1","type":"part","label":"Stick","mountpoints":[null],"fstype":"exfat","rm":true}]}"#;
const DRIVE: &str = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","tran":"usb","rm":false,"children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","rm":false,"label":"One","fstype":"ext4","mountpoints":["/media/one"]},{"name":"sdb2","path":"/dev/sdb2","type":"part","rm":false,"label":"Two","fstype":"ext4","mountpoints":["/media/two"]}]}]}"#;

fn listed(body: &str) -> Outcome {
    Outcome {
        code: 0,
        stdout: body.into(),
        stderr: String::new(),
    }
}

fn devices(body: &str) -> Devices {
    let mut d = Devices::new("host");
    d.poll(0);
    d.list_exited(&listed(body));
    d
}

fn at(d: &Devices, label: &str) -> usize {
    d.entries.iter().position(|e| e.label == label).unwrap()
}

fn moved(volume: &str) -> (String, Vec<Moved>) {
    let pane = Moved {
        side: 0,
        from: format!("{volume}/photos"),
        to: "/home/u".into(),
    };
    (volume.into(), vec![pane])
}

fn restored(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Restore(panes) => Some(
                panes
                    .iter()
                    .map(|p| p.from.clone())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            _ => None,
        })
        .collect()
}

/// Ejects the stick, as the window would after moving its pane, and judges the listing after gio.
fn eject_stick(d: &mut Devices, witness: &str, now: u64) -> Vec<Effect> {
    let index = at(d, "Stick");
    assert!(d
        .eject(index, now)
        .iter()
        .any(|e| matches!(e, Effect::Start(Proc::Eject, _))));
    d.moved = moved("/media/stick");
    d.eject_exited(&Outcome::code(0), now + 1);
    d.list_exited(&listed(witness))
}

#[test]
fn fx_eject_a_busy_refusal_returns_the_pane_and_an_eject_that_took_does_not() {
    let mut d = devices(STICK);
    let refused = eject_stick(&mut d, STICK, 10);
    assert_eq!(restored(&refused), ["/media/stick/photos"], "{refused:?}");
    let took = eject_stick(&mut d, STICK_GONE, 20);
    assert!(restored(&took).is_empty(), "{took:?}");
    assert!(d.moved.1.is_empty(), "nothing is left to return to");
}

#[test]
fn fx_eject_a_retry_withdraws_the_refusal_before_saying_its_own_verdict() {
    let mut d = devices(STICK);
    eject_stick(&mut d, STICK, 10);
    let took = eject_stick(&mut d, STICK_GONE, 20);
    let forget = took
        .iter()
        .position(|e| matches!(e, Effect::Forget(n) if n.key == "device.still_mounted"));
    let safe = took
        .iter()
        .position(|e| matches!(e, Effect::Message(n, false) if n.key == "device.safe"));
    assert!(forget.is_some() && forget < safe, "{took:?}");
}

#[test]
fn fx_eject_an_unconfirmed_eject_returns_nothing() {
    let mut d = devices(STICK);
    let index = at(&d, "Stick");
    d.eject(index, 10);
    d.moved = moved("/media/stick");
    d.eject_exited(&Outcome::code(0), 11);
    let late = d.tick(11 + super::devices::EJECT_VERDICT_MS);
    assert!(late
        .iter()
        .any(|e| matches!(e, Effect::Message(n, true) if n.key == "device.unconfirmed")));
    assert!(
        restored(&late).is_empty(),
        "an unknown state never moves the pane back: {late:?}"
    );
    assert!(d.moved.1.is_empty(), "no later verdict can return them");
}

#[test]
fn fx_eject_a_power_off_leg_killed_at_its_deadline_returns_only_volumes_it_never_tried() {
    for (volume, back) in [("/media/one", false), ("/media/two", true)] {
        let mut d = devices(DRIVE);
        let index = at(&d, "One");
        d.eject(index, 10);
        d.moved = moved(volume);
        let late = d.tick(10 + super::devices::POWER_OFF_WAIT_MS);
        assert!(
            late.iter()
                .any(|e| matches!(e, Effect::Kill(Proc::Unmount))),
            "{late:?}"
        );
        assert_eq!(!restored(&late).is_empty(), back, "{volume}: {late:?}");
    }
}

#[test]
fn fx_eject_an_eject_takes_down_its_whole_drive_and_an_unmount_only_its_volume() {
    let mut d = devices(DRIVE);
    let one = at(&d, "One");
    d.eject(one, 10);
    assert_eq!(
        d.release_volumes("/media/one"),
        ["/media/one", "/media/two"]
    );
    d.abandon();
    d.unmount(one);
    assert_eq!(d.release_volumes("/media/one"), ["/media/one"]);
    let mut stick = devices(STICK);
    let index = at(&stick, "Stick");
    stick.eject(index, 10);
    assert_eq!(stick.release_volumes("/media/stick"), ["/media/stick"]);
}

/// Powers off the USB drive from `label` and fails the leg named `failing` (1 or 2).
fn power_off(label: &str, volume: &str, failing: usize) -> Vec<Effect> {
    let mut d = devices(DRIVE);
    let index = at(&d, label);
    let first = d.eject(index, 10);
    assert!(
        first
            .iter()
            .any(|e| matches!(e, Effect::Start(Proc::Unmount, _))),
        "{first:?}"
    );
    d.moved = moved(volume);
    let mut out = Vec::new();
    for leg in 1..=failing {
        let code = if leg == failing { 1 } else { 0 };
        out = d.unmount_exited(&Outcome::code(code), 10 + leg as u64);
    }
    assert!(!d.releasing(), "the failed leg ends the chain");
    out
}

#[test]
fn fx_eject_a_failed_power_off_leg_returns_the_pane_only_while_its_volume_is_mounted() {
    assert_eq!(
        restored(&power_off("One", "/media/one", 1)),
        ["/media/one/photos"],
        "its own leg failed"
    );
    assert_eq!(
        restored(&power_off("One", "/media/two", 1)),
        ["/media/two/photos"],
        "its leg never ran"
    );
    assert!(
        restored(&power_off("One", "/media/one", 2)).is_empty(),
        "its leg already unmounted it"
    );
    assert_eq!(
        restored(&power_off("One", "/media/two", 2)),
        ["/media/two/photos"]
    );
}

#[test]
fn fx_eject_a_failed_unmount_returns_the_pane_and_a_finished_one_does_not() {
    let mut d = devices(STICK);
    let index = at(&d, "Stick");
    d.unmount(index);
    d.moved = moved("/media/stick");
    assert_eq!(
        restored(&d.unmount_exited(&Outcome::code(1), 5)),
        ["/media/stick/photos"]
    );
    d.unmount(index);
    d.moved = moved("/media/stick");
    assert!(restored(&d.unmount_exited(&Outcome::code(0), 6)).is_empty());
}

#[test]
fn fx_eject_an_abandoned_release_holds_nothing_and_the_next_one_runs() {
    let mut d = devices(STICK);
    let index = at(&d, "Stick");
    d.eject(index, 10);
    assert!(d.releasing() && d.running(Proc::Eject));
    d.abandon();
    assert!(!d.releasing() && !d.running(Proc::Eject) && !d.running(Proc::Unmount));
    assert!(d
        .eject(index, 20)
        .iter()
        .any(|e| matches!(e, Effect::Start(Proc::Eject, _))));
    d.abandon();
    let index = at(&d, "Stick");
    d.unmount(index);
    d.abandon();
    assert!(!d.releasing());
    let drive = &mut devices(DRIVE);
    let one = at(drive, "One");
    drive.eject(one, 30);
    drive.abandon();
    assert!(!drive.releasing() && !drive.running(Proc::Unmount));
    assert!(
        drive
            .unmount(one)
            .iter()
            .any(|e| matches!(e, Effect::Start(Proc::Unmount, _))),
        "the power-off chain is gone"
    );
}
