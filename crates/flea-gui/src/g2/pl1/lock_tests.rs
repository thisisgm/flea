//! EY-559: a refused lock is an error only while its container still has a cleartext mapping.
use super::devices::{Devices, Effect, Proc};
use super::process::Outcome;

const OPEN: &str = r#"{"blockdevices":[{"name":"loop0","path":"/dev/loop0","type":"loop","fstype":"crypto_LUKS","children":[{"name":"vault","path":"/dev/mapper/vault","type":"crypt","fstype":"ext4","label":"vault","mountpoints":["/run/media/gm/vault"]}]}]}"#;
const CLOSED: &str = r#"{"blockdevices":[{"name":"loop0","path":"/dev/loop0","type":"loop","fstype":"crypto_LUKS","mountpoints":[null]}]}"#;

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
        .filter_map(|effect| match effect {
            Effect::Message(note, true) => Some(note.english()),
            _ => None,
        })
        .collect()
}

fn unmounted() -> Devices {
    let mut devices = Devices::new("host");
    devices.poll(0);
    devices.list_exited(&listed(OPEN));
    let row = devices
        .entries
        .iter()
        .position(|entry| entry.device == "/dev/mapper/vault")
        .unwrap();
    devices.unmount(row);
    let effects = devices.unmount_exited(&Outcome::code(0), 10);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Start(Proc::Lock, job) if job.argv == ["udisksctl", "lock", "--block-device", "/dev/loop0"])));
    devices
}

#[test]
fn sol_luks_refusal_diagnosis_already_closed_mapping_finishes_silently() {
    let mut devices = unmounted();
    let outcome = Outcome {
        code: 1,
        stdout: String::new(),
        stderr: "Error locking /dev/loop0: GDBus.Error:org.freedesktop.UDisks2.Error.NotUnlocked: Device is not unlocked".into(),
    };
    let mut effects = devices.lock_exited(&outcome, 20);
    effects.extend(devices.list_exited(&listed(CLOSED)));
    assert!(
        messages(&effects).is_empty(),
        "a closed container is no lock failure: {:?}",
        messages(&effects)
    );
}

#[test]
fn sol_luks_refusal_diagnosis_native_failed_already_closed_mapping_is_silent() {
    let mut devices = unmounted();
    let outcome = Outcome {
        code: 1,
        stdout: String::new(),
        stderr: "Error locking /dev/nvme0n1p1: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: Device /dev/nvme0n1p1 is not unlocked\n".into(),
    };
    let mut effects = devices.lock_exited(&outcome, 20);
    effects.extend(devices.list_exited(&listed(CLOSED)));
    assert!(
        messages(&effects).is_empty(),
        "the observed Failed error is silent for a closed mapping: {:?}",
        messages(&effects)
    );
}

#[test]
fn sol_luks_refusal_diagnosis_still_open_mapping_reports_one_failure() {
    let mut devices = unmounted();
    let mut effects = devices.lock_exited(&Outcome::code(1), 20);
    effects.extend(devices.list_exited(&listed(OPEN)));
    assert_eq!(
        messages(&effects),
        ["vault was unmounted but could not be locked; close what is using it."]
    );
    devices.poll(30);
    assert!(messages(&devices.list_exited(&listed(OPEN))).is_empty());
}

#[test]
fn sol_luks_refusal_diagnosis_failure_waits_for_a_fresh_successful_listing() {
    let mut devices = unmounted();
    devices.poll(15);
    assert!(
        messages(&devices.lock_exited(&Outcome::code(1), 20)).is_empty(),
        "an exit status alone does not establish an open mapping"
    );
    assert!(
        messages(&devices.list_exited(&listed(OPEN))).is_empty(),
        "an earlier listing is stale"
    );
    devices.poll(30);
    let failed = Outcome {
        code: 1,
        stdout: OPEN.into(),
        stderr: "lsblk failed".into(),
    };
    assert!(
        messages(&devices.list_exited(&failed)).is_empty(),
        "partial failed output is not a witness"
    );
    devices.poll(40);
    assert!(messages(&devices.list_exited(&listed("not json"))).is_empty());
    devices.poll(50);
    assert_eq!(messages(&devices.list_exited(&listed(OPEN))).len(), 1);
}

#[test]
fn sol_luks_refusal_diagnosis_nonzero_without_not_unlocked_is_silent_when_closed() {
    let mut devices = unmounted();
    let mut effects = devices.lock_exited(&Outcome::code(-1), 20);
    effects.extend(devices.list_exited(&listed(CLOSED)));
    assert!(
        messages(&effects).is_empty(),
        "the mapping state decides even without a D-Bus error name"
    );
}

#[test]
fn sol_luks_refusal_diagnosis_other_container_cannot_supply_the_mapping() {
    let mut devices = unmounted();
    let mut effects = devices.lock_exited(&Outcome::code(1), 20);
    effects.extend(devices.list_exited(&listed(&OPEN.replace("loop0", "loop1"))));
    assert!(
        messages(&effects).is_empty(),
        "an unrelated open container is no failure of this lock"
    );
}

#[test]
fn sol_luks_refusal_diagnosis_queued_locks_keep_their_own_backing_and_label() {
    let mut devices = unmounted();
    let other = OPEN.replace("loop0", "loop1").replace("vault", "other");
    devices
        .lock_crypt("/dev/mapper/other", "other", &other)
        .unwrap();
    let effects = devices.lock_exited(&Outcome::code(1), 20);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Start(Proc::Lock, job) if job.argv.last().map(String::as_str) == Some("/dev/loop1"))));
    assert!(messages(&effects).is_empty());
    assert!(messages(&devices.lock_exited(&Outcome::code(1), 25)).is_empty());
    assert!(messages(&devices.list_exited(&listed(CLOSED))).is_empty());
    devices.poll(30);
    assert_eq!(
        messages(&devices.list_exited(&listed(&other))),
        ["other was unmounted but could not be locked; close what is using it."]
    );
}

#[test]
fn sol_luks_refusal_diagnosis_pending_check_polls_with_idle_polling_disabled() {
    let mut devices = unmounted();
    devices.polling = false;
    devices.tick(15);
    devices.lock_exited(&Outcome::code(1), 20);
    devices.list_exited(&listed("{}"));
    assert!(devices
        .tick(30)
        .iter()
        .any(|effect| matches!(effect, Effect::Start(Proc::List, _))));
    assert_eq!(messages(&devices.list_exited(&listed(OPEN))).len(), 1);
}

#[test]
fn sol_luks_refusal_diagnosis_listings_that_never_witness_keep_the_refusal() {
    let mut devices = unmounted();
    devices.polling = false;
    let failed = Outcome {
        code: 1,
        stdout: OPEN.into(),
        stderr: "lsblk failed".into(),
    };
    assert!(messages(&devices.lock_exited(&Outcome::code(1), 20)).is_empty());
    assert!(messages(&devices.list_exited(&failed)).is_empty());
    devices.poll(30);
    assert!(messages(&devices.list_exited(&listed("not json"))).is_empty());
    devices.poll(40);
    devices.tick(40 + super::devices::LIST_TIMEOUT_MS);
    assert_eq!(
        messages(&devices.list_exited(&listed(CLOSED))),
        ["vault was unmounted but could not be locked; close what is using it."],
        "a timed-out listing witnesses nothing, and the third such listing keeps the refusal"
    );
    assert!(
        devices.lock_checks.is_empty(),
        "the check ends, so polling may idle"
    );
}
