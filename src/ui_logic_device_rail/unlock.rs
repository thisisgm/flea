//! The unlock card's sentences and the udisksctl answers it parses; a passphrase goes only to the child's stdin.
use super::lsblk::{children, device_path, parse_nodes, text};
use super::{Note, RailEntry};
use crate::jsondoc::Json;

/// The card title for a locked row.
pub fn title(label: &str) -> Note {
    Note::new("device.unlock.title", &[label])
}

/// The status line while udisksctl runs.
pub fn unlocking_note(label: &str) -> Note {
    Note::new("device.unlock.unlocking", &[label])
}

/// The line under the field after a refused passphrase.
pub fn wrong_note() -> Note {
    Note::new("device.unlock.wrong", &[])
}

/// The status line after the volume unlocked and mounted.
pub fn unlocked_note(label: &str) -> Note {
    Note::new("device.unlock.unlocked", &[label])
}

/// The status line when a relock after unmount failed.
pub fn lock_note(label: &str) -> Note {
    Note::new("device.unlock.lock_failed", &[label])
}

/// The status line when the unlocked volume would not mount.
pub fn mount_note(label: &str) -> Note {
    Note::new("device.unlock.mount_failed", &[label])
}

/// The status line for any refusal other than the passphrase.
pub fn unlock_failed_note(label: &str) -> Note {
    Note::new("device.unlock.failed", &[label])
}

/// The cleartext device in "Unlocked /dev/sda1 as /dev/dm-0.", "" when absent.
pub fn parse_unlocked(output: &str) -> String {
    let found = output.match_indices("as").find_map(|(at, _)| {
        let after = &output[at + 2..];
        let gap = after.len() - after.trim_start().len();
        (gap > 0 && after[gap..].starts_with("/dev/")).then(|| &after[gap..])
    });
    let Some(rest) = found else { return String::new() };
    let token = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
    token.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':') || c.is_whitespace()).to_string()
}

/// The mount folder in "Mounted /dev/dm-0 at /run/media/gm/My Backup.", "" when absent.
pub fn parse_mounted(output: &str) -> String {
    for line in output.split(['\n', '\r', '\u{2028}', '\u{2029}']) {
        for (at, _) in line.match_indices("at") {
            let boundary = line[..at].chars().last().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
            let after = &line[at + 2..];
            let gap = after.len() - after.trim_start().len();
            if boundary && gap > 0 && after[gap..].starts_with('/') {
                let path = &after[gap..];
                return path.strip_suffix('.').unwrap_or(path).to_string();
            }
        }
    }
    String::new()
}

/// The D-Bus error name in udisksctl's stderr, "" when none.
pub fn parse_dbus_error(output: &str) -> String {
    let Some(at) = output.find("GDBus.Error:") else { return String::new() };
    let rest = &output[at + "GDBus.Error:".len()..];
    rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')).collect()
}

/// The unlock argv; udisksctl opens --key-file as a path, so stdin is /dev/stdin.
pub fn unlock_argv(device: &str) -> Vec<String> {
    ["udisksctl", "unlock", "--block-device", device, "--key-file", "/dev/stdin"].iter().map(|s| s.to_string()).collect()
}

/// The lock argv for a container.
pub fn lock_argv(backing: &str) -> Vec<String> {
    ["udisksctl", "lock", "--block-device", backing].iter().map(|s| s.to_string()).collect()
}

/// Only UDisks2.Error.Failed reads as a wrong passphrase; every other refusal could not unlock.
pub fn refusal_is_wrong(stderr: &str) -> bool {
    parse_dbus_error(stderr) == "org.freedesktop.UDisks2.Error.Failed"
}

/// The refusal sentence and whether it belongs under the field rather than in the status bar.
pub fn refusal_note(stderr: &str, label: &str) -> Note {
    if refusal_is_wrong(stderr) { wrong_note() } else { unlock_failed_note(label) }
}

/// The locked row naming a device, None before the rail lists it.
pub fn locked_row<'a>(entries: &'a [RailEntry], device: &str) -> Option<&'a RailEntry> {
    entries.iter().find(|r| r.locked && r.device == device)
}

/// The locked row wherever the list lives, the rail first.
pub fn naming_row<'a>(primary: &'a [RailEntry], fallback: &'a [RailEntry], device: &str) -> Option<&'a RailEntry> {
    locked_row(primary, device).or_else(|| locked_row(fallback, device))
}

/// The container format as the card names it: LUKS1, LUKS2, LUKS, or the crypto type uppercased.
pub fn crypto_name(fstype: &str, fsver: &str) -> String {
    if fstype.to_lowercase() == "crypto_luks" {
        return match fsver {
            "2" => "LUKS2",
            "1" => "LUKS1",
            _ => "LUKS",
        }
        .into();
    }
    let crypto = fstype.get(..7).is_some_and(|head| head.eq_ignore_ascii_case("crypto_"));
    let name = if crypto { &fstype[7..] } else { fstype };
    if name.is_empty() { "LUKS".into() } else { name.to_uppercase() }
}

/// The name line under the title, one template per shape.
pub fn subtitle(label: &str, size_text: &str, crypto: &str) -> Note {
    match (size_text.is_empty(), crypto.is_empty()) {
        (false, false) => Note::new("device.unlock.subtitle_three", &[label, size_text, crypto]),
        (false, true) => Note::new("device.unlock.subtitle_two", &[label, size_text]),
        (true, false) => Note::new("device.unlock.subtitle_two", &[label, crypto]),
        (true, true) => Note::plain(label),
    }
}

/// The container under an open crypt device, "" when the listing no longer shows it.
pub fn backing_for(body: &str, crypt_device: &str) -> String {
    let Some(nodes) = parse_nodes(body) else { return String::new() };
    nodes.iter().map(|n| backing_in(n, crypt_device)).find(|f| !f.is_empty()).unwrap_or_default()
}

fn backing_in(node: &Json, want: &str) -> String {
    for kid in children(node) {
        if text(kid, "type") == "crypt" && device_path(kid) == want {
            return device_path(node);
        }
        let deeper = backing_in(kid, want);
        if !deeper.is_empty() {
            return deeper;
        }
    }
    String::new()
}
