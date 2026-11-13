use super::devices::{Devices, Effect};
use super::process::Outcome;
use super::unlock::{Card, Effect as CardEffect, Proc};

const LOCKED: &str = r#"{"blockdevices":[{"name":"nvme1n1p3","path":"/dev/nvme1n1p3","type":"part","fstype":"BitLocker","label":"Backup","size":500000000000,"mountpoints":[null]}]}"#;

#[test]
fn bitlocker_rail_activation_requests_unlock_instead_of_gio_mount() {
    for show_unmounted in [false, true] {
        let mut devices = Devices::new("fixture");
        devices.show_unmounted = show_unmounted;
        devices.poll(0);
        devices.list_exited(&Outcome {
            code: 0,
            stdout: LOCKED.into(),
            stderr: String::new(),
        });
        assert_eq!(devices.entries.len(), 1);
        assert!(matches!(
            &devices.activate(0)[..],
            [Effect::UnlockRequested { device, label, size: 500_000_000_000, fstype }]
                if device == "/dev/nvme1n1p3" && label == "Backup" && fstype == "BitLocker"
        ));
    }
}

#[test]
fn bitlocker_unlock_mounts_the_returned_cleartext_device() {
    let mut card = Card::new();
    card.open_for("/dev/nvme1n1p3", "Backup", 500_000_000_000, "BitLocker", "");
    card.type_text("fixture passphrase");
    card.submit(0);
    card.started(Proc::Unlock);
    let effects = card.exited(
        Proc::Unlock,
        &Outcome {
            code: 0,
            stdout: "Unlocked /dev/nvme1n1p3 as /dev/dm-7.\n".into(),
            stderr: String::new(),
        },
        1,
    );
    assert!(matches!(
        &effects[..],
        [CardEffect::Start(Proc::Mount, job)]
            if job.argv == ["udisksctl", "mount", "--block-device", "/dev/dm-7"]
    ));
}
