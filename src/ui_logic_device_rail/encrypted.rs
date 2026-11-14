//! Filesystem signatures handled by the encrypted-container unlock path.

/// Keep the existing crypto family and recognize lsblk's BitLocker signature in any letter case.
pub fn is_container(fstype: &str) -> bool {
    fstype
        .get(..7)
        .is_some_and(|head| head.eq_ignore_ascii_case("crypto_"))
        || fstype.eq_ignore_ascii_case("bitlocker")
}
