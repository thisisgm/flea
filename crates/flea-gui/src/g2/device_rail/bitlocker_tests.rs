//! A BitLocker card hands udisks the recovery key in the dashed form cryptsetup reads, however Windows let it be typed.
use super::unlock::{Card, Effect};

// cryptsetup's public bitlk-aes-cbc-128 test vector, never a real key.
const DASHED: &str = "042647-302313-590458-071500-554323-116567-412181-516978";

fn stdin_for(fstype: &str, typed: &str) -> Vec<u8> {
    let mut card = Card::new();
    card.open_for("/dev/sdb1", "Games", 0, fstype, "2");
    card.type_text(typed);
    let effects = card.submit(0);
    let Some(Effect::Start(_, job)) = effects.iter().find(|e| matches!(e, Effect::Start(..)))
    else {
        panic!("submit starts udisksctl");
    };
    job.stdin
        .as_ref()
        .map(|s| s.expose().to_vec())
        .unwrap_or_default()
}

#[test]
fn a_recovery_key_typed_without_dashes_reaches_udisks_dashed() {
    let bare = DASHED.replace('-', "");
    assert_eq!(
        stdin_for("BitLocker", &bare),
        DASHED.as_bytes(),
        "48 bare digits regroup into eight dashed groups"
    );
    assert_eq!(
        stdin_for("BitLocker", &DASHED.replace('-', " ")),
        DASHED.as_bytes(),
        "spaced groups regroup"
    );
    assert_eq!(
        stdin_for("BitLocker", &format!(" {DASHED} ")),
        DASHED.as_bytes(),
        "a pasted key loses its edge spaces"
    );
    assert_eq!(
        stdin_for("BitLocker", DASHED),
        DASHED.as_bytes(),
        "a dashed key reaches udisks unchanged"
    );
}

#[test]
fn only_a_bitlocker_recovery_key_is_regrouped() {
    let bare = DASHED.replace('-', "");
    assert_eq!(
        stdin_for("crypto_LUKS", &bare),
        bare.as_bytes(),
        "a LUKS passphrase of 48 digits is the user's own"
    );
    assert_eq!(
        stdin_for("BitLocker", "pass word 1"),
        b"pass word 1",
        "a BitLocker password is never touched"
    );
    assert_eq!(
        stdin_for("BitLocker", &bare[..47]),
        bare[..47].as_bytes(),
        "47 digits are a password, not a key"
    );
}
