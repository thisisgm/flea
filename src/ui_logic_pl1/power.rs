//! Power-off, eject and mount-error rules for one drive, read off lsblk, sysfs and gio stderr.
use super::devices::{dedupe, holds_root, is_system_mount_only};
use super::lsblk::{children, device_path, has, mount_of, parse_nodes};
use super::{Kind, Note, RailEntry};
use crate::jsondoc::Json;

/// The whole-disk node a partition powers off through, or "" for a path that names none.
pub fn power_off_disk(device: &str) -> String {
    for split in (0..device.len()).rev().filter(|at| device.is_char_boundary(*at)) {
        let Some(end) = disk_name_end(&device[..split]) else { continue };
        if end != split {
            continue;
        }
        let rest = &device[split..];
        let digits = rest.strip_prefix('p').unwrap_or(rest);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return device[..split].to_string();
        }
    }
    if disk_name_end(device).is_some_and(|end| end == device.len()) {
        return device.to_string();
    }
    String::new()
}

fn disk_name_end(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("/dev/")?;
    let alpha = |s: &str, prefix: &str| {
        let tail = s.strip_prefix(prefix)?;
        let n = tail.bytes().take_while(u8::is_ascii_lowercase).count();
        (n > 0).then_some(prefix.len() + n)
    };
    let digits = |s: &str, prefix: &str| {
        let tail = s.strip_prefix(prefix)?;
        let n = tail.bytes().take_while(u8::is_ascii_digit).count();
        (n > 0).then_some(prefix.len() + n)
    };
    let nvme = |s: &str| {
        let first = digits(s, "nvme")?;
        let tail = s[first..].strip_prefix('n')?;
        let n = tail.bytes().take_while(u8::is_ascii_digit).count();
        (n > 0).then_some(first + 1 + n)
    };
    let len = alpha(rest, "sd")
        .or_else(|| alpha(rest, "hd"))
        .or_else(|| alpha(rest, "vd"))
        .or_else(|| nvme(rest))
        .or_else(|| digits(rest, "mmcblk"))
        .or_else(|| digits(rest, "loop"))?;
    Some(5 + len)
}

/// The sysfs block name of a disk path, "" when it is not under /dev.
pub fn sys_base(disk: &str) -> String {
    disk.strip_prefix("/dev/").filter(|s| !s.is_empty() && !s.contains('\n')).unwrap_or("").to_string()
}

/// The written-sector counter, field seven of /sys/block/<disk>/stat, "" when short.
pub fn written_sectors(body: &str) -> String {
    let fields: Vec<&str> = body.split_whitespace().collect();
    fields.get(6).map(|s| s.to_string()).unwrap_or_default()
}

/// A device and the folder it is mounted on, queued for a power-off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queued {
    pub device: String,
    pub path: String,
}

/// Every mounted node under one disk, hidden ones too; an unparsable listing queues the mounted rail volumes.
pub fn power_off_queue(listing: &str, disk: &str, entries: &[RailEntry]) -> Vec<Queued> {
    let rows: Vec<RailEntry> = match parse_nodes(listing) {
        None => entries
            .iter()
            .filter(|r| r.kind == Kind::Volume && r.mounted && power_off_disk(&r.device) == disk)
            .map(|r| RailEntry { device: r.device.clone(), path: r.path.clone(), ..RailEntry::default() })
            .collect(),
        Some(nodes) => {
            let mut out = Vec::new();
            for node in nodes.iter().filter(|n| has(n, "name") && power_off_disk(&device_path(n)) == disk) {
                collect_mounted(node, &mut out);
            }
            out
        }
    };
    dedupe(rows).into_iter().map(|r| Queued { device: r.device, path: r.path }).collect()
}

fn collect_mounted(node: &Json, out: &mut Vec<RailEntry>) {
    let point = mount_of(node);
    if !point.is_empty() && !holds_root(node) && !is_system_mount_only(node) {
        out.push(RailEntry { device: device_path(node), path: point, ..RailEntry::default() });
    }
    for kid in children(node) {
        collect_mounted(kid, out);
    }
}

/// Every top lsblk node whose tree holds `device`, so an open crypt or a spanned array names each drive it sits on.
/// {"blockdevices":[{"path":"/dev/sdb","children":[{"path":"/dev/sdb2","children":[{"path":"/dev/mapper/luks-x"}]}]}]}
pub fn drives_holding(listing: &str, device: &str) -> Vec<String> {
    let Some(nodes) = parse_nodes(listing) else { return Vec::new() };
    nodes.iter().filter(|n| holds_device(n, device)).map(device_path).collect()
}

fn holds_device(node: &Json, device: &str) -> bool {
    device_path(node) == device || children(node).iter().any(|kid| holds_device(kid, device))
}

/// The exit the loop eject chain uses when the loop device could not be detached.
pub const DETACH_FAILED_EXIT: i32 = 3;

/// The release argv: gio eject for a drive, an unmount and loop-delete chain for an image.
pub fn eject_command(entry: &RailEntry) -> Vec<String> {
    let owned = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let path = entry.path.as_str();
    if !entry.loop_device {
        return owned(&["gio", "mount", "-e", path]);
    }
    let Some(name) = loop_name(&entry.device) else { return owned(&["gio", "mount", "-u", path]) };
    let script = format!(
        "{}{}{}{}{}",
        "autoclear=$(cat -- \"$4\" 2>/dev/null) || autoclear=; before=$(cat -- \"$3\" 2>/dev/null); ",
        "status=0; gio mount -u \"$1\" || status=1; if [ \"$autoclear\" != 0 ]; then exit \"$status\"; fi; ",
        "after=$(cat -- \"$3\" 2>/dev/null); if [ -n \"$before\" ] && [ \"$after\" = \"$before\" ]; then ",
        "udisksctl loop-delete --no-user-interaction -b \"$2\" || exit ",
        format_args!("{DETACH_FAILED_EXIT}; fi; exit \"$status\""),
    );
    let disk = format!("/dev/{name}");
    let backing = format!("/sys/block/{name}/loop/backing_file");
    let autoclear = format!("/sys/block/{name}/loop/autoclear");
    owned(&["sh", "-c", &script, "sh", path, &disk, &backing, &autoclear])
}

fn loop_name(device: &str) -> Option<&str> {
    let rest = device.strip_prefix("/dev/")?;
    let digits = rest.strip_prefix("loop")?.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let (name, tail) = rest.split_at(4 + digits);
    let part = tail.strip_prefix('p').map(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    (tail.is_empty() || part == Some(true)).then_some(name)
}

/// Which gio operation failed, for the sentence that names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Mount,
    Unmount,
    Eject,
}

/// The sentence for a failed gio call, None when it succeeded or the volume was already mounted.
pub fn mount_error(op: Op, exit_code: i32, stderr: &str, label: &str) -> Option<Note> {
    if exit_code == 0 {
        return None;
    }
    let text = stderr.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| text.contains(w));
    let key = if any(&["already mounted"]) {
        return None;
    } else if any(&["busy", "in use"]) {
        "device.busy"
    } else if any(&["not authorized", "not allowed", "refused", "dismissed", "no authentication", "auth"]) {
        match op {
            Op::Unmount => "device.unmount_refused",
            Op::Eject => "device.eject_refused",
            Op::Mount => "device.mount_refused",
        }
    } else if any(&["no volume", "no medium", "no media", "empty", "no device"]) {
        if any(&["medium", "media", "empty"]) { "device.no_disc" } else { "device.not_mountable" }
    } else if op == Op::Mount && (any(&DAMAGED) || cant_read_superblock(&text)) {
        "device.damaged"
    } else {
        match op {
            Op::Unmount => "device.unmount_failed",
            Op::Eject => "device.eject_failed",
            Op::Mount => "device.mount_failed",
        }
    };
    Some(Note::new(key, &[label]))
}

const DAMAGED: [&str; 4] = ["wrong fs type", "bad superblock", "unknown filesystem type", "structure needs cleaning"];

fn cant_read_superblock(text: &str) -> bool {
    text.match_indices("can").any(|(at, _)| {
        let mut rest = text[at + 3..].chars();
        rest.next().is_some_and(|c| c != '\n') && rest.as_str().starts_with("t read superblock")
    })
}

/// gio's own reason as one detail line, its gio, uri and "Error mounting X at Y" leads stripped.
pub fn mount_detail(stderr: &str) -> String {
    let joined = stderr.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    let text = joined.strip_prefix("gio:").map(str::trim_start).unwrap_or(&joined);
    let text = strip_uri_lead(text);
    strip_error_lead(text).trim().to_string()
}

fn strip_uri_lead(text: &str) -> &str {
    let scheme = text.find("://").filter(|&mark| {
        let head = &text[..mark];
        head.starts_with(|c: char| c.is_ascii_alphabetic())
            && head.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    });
    let Some(mark) = scheme else { return text };
    let Some(space) = text.find(char::is_whitespace) else { return text };
    if space < mark + 4 || !text[..space].ends_with(':') {
        return text;
    }
    text[space..].trim_start()
}

fn strip_error_lead(text: &str) -> &str {
    let lead = ["Error mounting ", "Error unmounting ", "Error ejecting "];
    let Some(rest) = lead.iter().find_map(|l| text.strip_prefix(l)) else { return text };
    let token = rest.find(char::is_whitespace).unwrap_or(rest.len());
    for end in (1..=token).rev().filter(|e| rest.is_char_boundary(*e)) {
        let after = &rest[end..];
        if let Some(tail) = after.strip_prefix(" at ") {
            let found = tail.match_indices(':').find(|(at, _)| tail[at + 1..].starts_with(char::is_whitespace));
            if let Some((at, _)) = found {
                return tail[at + 1..].trim_start();
            }
        }
        if let Some(tail) = after.strip_prefix(':') {
            if tail.starts_with(char::is_whitespace) {
                return tail.trim_start();
            }
        }
    }
    text
}
