//! tests/js/loopdevices.js's ejectMessage cases against the Devices machine: the verdict comes from the listing after gio, never its exit code.
use super::devices::{Devices, Effect, Proc};
use super::process::Outcome;

const MOUNTED_ISO: &str = concat!(
    r#"{"blockdevices":["#,
    r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
    r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
    r#"{"name":"loop0","path":"/dev/loop0","label":null,"mountpoints":[null],"rm":false,"size":6224838656,"type":"loop","model":null,"#,
    r#""children":[{"name":"loop0p1","path":"/dev/loop0p1","label":"OMARCHY_202608","mountpoints":["/run/media/user/OMARCHY_202608"],"rm":false,"size":6224838656,"type":"part","model":null},"#,
    r#"{"name":"loop0p2","path":"/dev/loop0p2","label":null,"mountpoints":[null],"rm":false,"size":24117248,"type":"part","model":null}]}"#,
    r#"]}"#,
);
const GONE: &str =
    r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","mountpoints":["/"]}]}"#;
const KEPT: &str = r#"{"blockdevices":[{"name":"loop0","path":"/dev/loop0","children":[{"name":"loop0p1","path":"/dev/loop0p1","mountpoints":["/run/media/user/OMARCHY_202608"]}]}]}"#;
const STALE: &str = r#"{"blockdevices":[{"name":"loop0","children":[{"name":"loop0p1","mountpoints":["/old-mount"]}]}]}"#;

fn listed(body: &str) -> Outcome {
    Outcome {
        code: 0,
        stdout: body.into(),
        stderr: String::new(),
    }
}

fn messages(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| {
            if let Effect::Message(n, error) = e {
                Some(format!("{}|{error}", n.english()))
            } else {
                None
            }
        })
        .collect()
}

/// Ejects the loop partition, lets a listing started before the exit land stale, then judges the next one.
fn eject_message(code: i32, listing: &str) -> String {
    let mut d = Devices::new("host");
    d.poll(0);
    d.list_exited(&listed(MOUNTED_ISO));
    let index = d
        .entries
        .iter()
        .position(|e| e.device == "/dev/loop0p1")
        .expect("the loop partition is a row");
    let effects = d.eject(index, 10);
    assert!(effects.iter().any(|e| matches!(e, Effect::Start(Proc::Eject, job) if job.argv.iter().any(|a| a == "/dev/loop0"))));
    assert!(
        d.poll(15)
            .iter()
            .any(|e| matches!(e, Effect::Start(Proc::List, _))),
        "a listing already under way"
    );
    assert!(
        d.eject_exited(&Outcome::code(code), 20).is_empty(),
        "the running listing is not the witness"
    );
    assert!(
        messages(&d.list_exited(&listed(STALE))).is_empty(),
        "a listing started before gio exited settles nothing"
    );
    assert!(d
        .poll(30)
        .iter()
        .any(|e| matches!(e, Effect::Start(Proc::List, _))));
    messages(&d.list_exited(&listed(listing))).join(";")
}

#[test]
fn exit_3_with_an_unmounted_loop_reports_the_failed_detach() {
    assert_eq!(
        eject_message(3, GONE),
        "Unmounted OMARCHY_202608, but its loop device is still attached.|true"
    );
}

#[test]
fn exit_1_with_the_mount_gone_is_judged_ejected_by_the_next_listing() {
    assert_eq!(eject_message(1, GONE), "Ejected OMARCHY_202608.|false");
}

#[test]
fn exit_1_with_the_mount_kept_reports_still_mounted() {
    assert_eq!(
        eject_message(1, KEPT),
        "OMARCHY_202608 is still mounted; close what is using it.|true"
    );
}

#[test]
fn exit_0_with_the_mount_gone_reports_ejected_without_an_unplug_line() {
    assert_eq!(eject_message(0, GONE), "Ejected OMARCHY_202608.|false");
}

#[test]
fn exit_0_with_the_mount_kept_never_reports_success() {
    assert_eq!(
        eject_message(0, KEPT),
        "OMARCHY_202608 is still mounted; close what is using it.|true"
    );
}

#[test]
fn exit_0_with_another_partition_mounted_reports_only_the_target_ejected() {
    assert_eq!(
        eject_message(0, &KEPT.replace("loop0p1", "loop0p2")),
        "Ejected OMARCHY_202608.|false"
    );
}

#[test]
fn a_second_eject_while_one_is_judged_is_refused() {
    let mut d = Devices::new("host");
    d.poll(0);
    d.list_exited(&listed(MOUNTED_ISO));
    let index = d
        .entries
        .iter()
        .position(|e| e.device == "/dev/loop0p1")
        .unwrap();
    d.eject(index, 10);
    let again = d.eject(index, 11);
    assert_eq!(
        messages(&again),
        ["Still ejecting OMARCHY_202608; wait for its result|false"]
    );
}

#[test]
fn no_witness_by_the_bound_says_do_not_unplug() {
    let mut d = Devices::new("host");
    d.poll(0);
    d.list_exited(&listed(MOUNTED_ISO));
    let index = d
        .entries
        .iter()
        .position(|e| e.device == "/dev/loop0p1")
        .unwrap();
    d.eject(index, 10);
    d.eject_exited(&Outcome::code(0), 20);
    let effects = d.tick(20 + super::devices::EJECT_VERDICT_MS);
    assert!(
        messages(&effects)
            .iter()
            .any(|m| m.ends_with("|true") && m.contains("do not unplug")),
        "{:?}",
        messages(&effects)
    );
    assert!(!d.releasing());
}

const OPEN_CRYPT: &str = concat!(
    r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"mountpoints":[null],"#,
    r#""children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","fstype":"crypto_LUKS","mountpoints":[null],"#,
    r#""children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","type":"crypt","mountpoints":["/run/media/gm/vault"]}]}]}]}"#,
);
const LOOP_CRYPT: &str = concat!(
    r#"{"blockdevices":[{"name":"cap-loop0","path":"/dev/loop0","mountpoints":[null],"rm":true,"type":"loop","fstype":"crypto_LUKS","#,
    r#""children":[{"name":"luks-6f1c","path":"/dev/mapper/luks-6f1c","label":"S1LUKS","mountpoints":["/run/media/gm/S1LUKS"],"#,
    r#""rm":true,"type":"crypt","fstype":"vfat"}]}]}"#,
);

fn lock_argv(effects: &[Effect]) -> Option<String> {
    effects.iter().find_map(|e| {
        if let Effect::Start(Proc::Lock, job) = e {
            Some(job.argv.join(" "))
        } else {
            None
        }
    })
}

fn listed_devices(body: &str) -> Devices {
    let mut d = Devices::new("host");
    d.poll(0);
    d.list_exited(&listed(body));
    d
}

#[test]
fn an_unmount_locks_from_the_listing_it_already_holds_and_a_refusal_is_one_attempt() {
    let mut d = listed_devices(OPEN_CRYPT);
    let index = d
        .entries
        .iter()
        .position(|e| e.device == "/dev/mapper/luks-vault")
        .expect("the open crypt is a row");
    let effects = d.unmount(index);
    assert!(effects.iter().any(|e| matches!(e, Effect::Start(Proc::Unmount, job) if job.argv.join(" ") == "gio mount -u /run/media/gm/vault")));
    let effects = d.unmount_exited(&Outcome::code(0), 10);
    assert_eq!(
        lock_argv(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/sdb1")
    );
    let effects = d.lock_exited(&Outcome::code(1), 20);
    assert!(
        messages(&effects).is_empty(),
        "the lock exit awaits a fresh mapping snapshot"
    );
    let effects = d.list_exited(&listed(OPEN_CRYPT));
    assert_eq!(
        messages(&effects),
        ["vault was unmounted but could not be locked; close what is using it.|true"]
    );
    assert!(
        lock_argv(&effects).is_none(),
        "a container gvfs already locked gets one refused attempt"
    );
}

/// Ejects the loop-backed crypt and judges the given listing as its witness.
fn eject_crypt(witness: &str) -> Vec<Effect> {
    let mut d = listed_devices(LOOP_CRYPT);
    let index = d
        .entries
        .iter()
        .position(|e| e.device == "/dev/mapper/luks-6f1c")
        .expect("the crypt leaf is a row");
    d.eject(index, 10);
    d.eject_exited(&Outcome::code(0), 20);
    d.list_exited(&listed(witness))
}

#[test]
fn a_crypt_the_judged_listing_still_shows_is_locked() {
    let witness = LOOP_CRYPT.replace(
        r#""mountpoints":["/run/media/gm/S1LUKS"]"#,
        r#""mountpoints":[null]"#,
    );
    let effects = eject_crypt(&witness);
    assert_eq!(
        lock_argv(&effects).as_deref(),
        Some("udisksctl lock --block-device /dev/loop0"),
        "{:?}",
        messages(&effects)
    );
}

#[test]
fn a_container_gvfs_already_locked_gets_no_lock() {
    let effects = eject_crypt(GONE);
    assert!(lock_argv(&effects).is_none());
    assert!(
        messages(&effects).iter().any(|m| m.ends_with("|false")),
        "{:?}",
        messages(&effects)
    );
}

#[test]
fn one_release_at_a_time_across_eject_and_unmount() {
    let listed_iso = || {
        let mut d = Devices::new("host");
        d.poll(0);
        d.list_exited(&listed(MOUNTED_ISO));
        let at = d
            .entries
            .iter()
            .position(|e| e.device == "/dev/loop0p1")
            .expect("the loop partition is a row");
        (d, at)
    };
    let quiesces = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|e| matches!(e, Effect::Quiesce(_)))
            .count()
    };
    let (mut d, at) = listed_iso();
    assert_eq!(quiesces(&d.unmount(at)), 1);
    let second = d.eject(at, 10);
    assert_eq!(
        quiesces(&second),
        0,
        "a running unmount keeps its held child"
    );
    assert_eq!(
        messages(&second),
        ["Still unmounting OMARCHY_202608; wait for its result|false"]
    );
    let (mut d, at) = listed_iso();
    assert_eq!(quiesces(&d.eject(at, 10)), 1);
    let second = d.unmount(at);
    assert_eq!(quiesces(&second), 0, "an armed eject keeps its held child");
    assert_eq!(
        messages(&second),
        ["Still ejecting OMARCHY_202608; wait for its result|false"]
    );
}

// One stick holding an open crypt and a plain partition, beside a second stick.
const CRYPT_BESIDE: &str = concat!(
    r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"mountpoints":[null],"children":["#,
    r#"{"name":"sdb1","path":"/dev/sdb1","type":"part","rm":true,"label":"Vault","fstype":"crypto_LUKS","mountpoints":[null]"#,
    r#","children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","type":"crypt","rm":true,"fstype":"ext4","mountpoints":["/run/media/gm/Vault"]}]"#,
    r#"},{"name":"sdb2","path":"/dev/sdb2","type":"part","rm":true,"label":"Stick","fstype":"ext4","mountpoints":["/run/media/gm/Stick"]}]},"#,
    r#"{"name":"sdc","path":"/dev/sdc","type":"disk","rm":true,"mountpoints":[null],"children":["#,
    r#"{"name":"sdc1","path":"/dev/sdc1","type":"part","rm":true,"label":"Other","fstype":"ext4","mountpoints":["/run/media/gm/Other"]}]}]}"#,
);
const LUKS_CHILD: &str = r#","children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","type":"crypt","rm":true,"fstype":"ext4","mountpoints":["/run/media/gm/Vault"]}]"#;

fn at(d: &Devices, label: &str) -> usize {
    d.entries
        .iter()
        .position(|e| e.label == label)
        .unwrap_or_else(|| panic!("the listing shows {label}"))
}

#[test]
fn an_eject_holds_an_open_crypt_on_its_drive_and_a_crypt_eject_holds_the_drive() {
    let locked = CRYPT_BESIDE.replace(LUKS_CHILD, "");
    assert_ne!(locked, CRYPT_BESIDE, "the fixture can lock its crypt");
    for (source, neighbour) in [("Stick", "Vault"), ("Vault", "Stick")] {
        let mut d = listed_devices(CRYPT_BESIDE);
        let started = d.eject(at(&d, source), 1);
        assert!(
            started
                .iter()
                .any(|e| matches!(e, Effect::Start(Proc::Eject, _))),
            "{source} ejects: {started:?}"
        );
        let held = [format!(
            "Still ejecting {source}; wait for its result|false"
        )];
        assert_eq!(messages(&d.activate(at(&d, neighbour))), held);
        let other = d.activate(at(&d, "Other"));
        assert!(
            other
                .iter()
                .any(|e| matches!(e, Effect::Opened(path) if path == "/run/media/gm/Other")),
            "a volume on another drive still opens: {other:?}"
        );
        if source == "Vault" {
            // A listing that lands mid-eject may already show the crypt locked and gone.
            d.list_exited(&listed(&locked));
            assert_eq!(messages(&d.activate(at(&d, "Stick"))), held);
        }
    }
}

#[test]
fn a_release_says_its_notice_before_the_quiesce_on_both_paths() {
    let listing = concat!(
        r#"{"blockdevices":["#,
        r#"{"name":"sda1","path":"/dev/sda1","type":"part","label":"128GB","mountpoints":["/media/128GB"],"fstype":"exfat","rm":true,"size":128000000000},"#,
        r#"{"name":"sdb","path":"/dev/sdb","type":"disk","rm":false,"size":2000000000,"children":["#,
        r#"{"name":"sdb1","path":"/dev/sdb1","type":"part","label":"Vault","mountpoints":["/media/vault"],"fstype":"ext4","rm":false,"size":2000000000}]}"#,
        r#"]}"#,
    );
    for (device, path) in [("/dev/sda1", "eject"), ("/dev/sdb1", "power off")] {
        let mut d = Devices::new("host");
        d.poll(0);
        d.list_exited(&listed(listing));
        let at = d
            .entries
            .iter()
            .position(|e| e.device == device)
            .expect("the volume is a row");
        let effects = d.eject(at, 10);
        assert_eq!(
            d.power_disk.is_empty(),
            path == "eject",
            "{path} takes its own branch"
        );
        let notice = effects.iter().position(
            |e| matches!(e, Effect::Message(note, false) if note.key == "device.ejecting"),
        );
        let quiesce = effects.iter().position(|e| matches!(e, Effect::Quiesce(_)));
        // A quiesce failure is said after the notice, so the notice must not come after it.
        assert!(
            notice.is_some() && quiesce.is_some() && notice < quiesce,
            "{path}: {notice:?} before {quiesce:?}"
        );
    }
}
