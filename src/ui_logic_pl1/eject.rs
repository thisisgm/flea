//! Whether an eject took, judged from a fresh lsblk listing, and what Ctrl+E releases.
use super::lsblk::{children, has, parse_nodes, raw_name, text};
use super::menu::{rail_menu, MenuRow, RailAction};
use super::mounts::holding;
use super::protocols::scheme_of;
use super::{Group, Kind, Note, RailEntry};
use crate::jsondoc::Json;

/// What a fresh listing says about the drive that was released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Safe,
    Mounted,
    Unknown,
    Attached,
}

/// The top-level node holding a device: None when the body is no usable listing, Some(None) when no node holds it.
pub fn disk_of(body: &str, device: &str) -> Option<Option<Json>> {
    let nodes = parse_nodes(body)?;
    if nodes.is_empty() {
        return None;
    }
    let name = kernel_name(device);
    if name.is_empty() {
        return None;
    }
    Some(nodes.into_iter().find(|n| holds(n, name)))
}

/// The leaf after the last slash, which is what lsblk's NAME column spells.
pub fn kernel_name(device: &str) -> &str {
    device.rsplit('/').next().unwrap_or(device)
}

fn holds(node: &Json, name: &str) -> bool {
    raw_name(node) == name || children(node).iter().any(|k| holds(k, name))
}

fn in_use(node: &Json) -> bool {
    node.get("mountpoints").and_then(Json::as_array).unwrap_or(&[]).iter().any(|p| match p {
        Json::Null => false,
        Json::Str(s) => !s.is_empty(),
        _ => true,
    })
}

fn any_mounted(node: &Json) -> bool {
    in_use(node) || children(node).iter().any(any_mounted)
}

/// Safe when the disk is gone or nothing under it is mounted, swap included; gio's exit code is never the witness.
pub fn verdict(body: &str, device: &str) -> Verdict {
    match disk_of(body, device) {
        None => Verdict::Unknown,
        Some(None) => Verdict::Safe,
        Some(Some(disk)) if any_mounted(&disk) => Verdict::Mounted,
        Some(Some(_)) => Verdict::Safe,
    }
}

/// The labels still mounted on the same disk, empty when the row itself is still mounted.
pub fn blockers(body: &str, device: &str) -> Vec<String> {
    let Some(Some(disk)) = disk_of(body, device) else { return Vec::new() };
    let mut mounted = Vec::new();
    collect_mounted(&disk, &mut mounted);
    let name = kernel_name(device);
    if mounted.iter().any(|(n, _)| n == name) {
        return Vec::new();
    }
    mounted.into_iter().map(|(_, label)| label).collect()
}

fn collect_mounted(node: &Json, out: &mut Vec<(String, String)>) {
    if in_use(node) {
        let label = if has(node, "label") { text(node, "label") } else { raw_name(node) };
        out.push((raw_name(node), label));
    }
    for kid in children(node) {
        collect_mounted(kid, out);
    }
}

/// The status sentence for a verdict, and whether it reads as an error.
pub fn sentence(verdict: Verdict, label: &str, others: &[String], loop_device: bool) -> (Note, bool) {
    match verdict {
        Verdict::Safe => (Note::new(if loop_device { "device.safe_image" } else { "device.safe" }, &[label]), false),
        Verdict::Attached => (Note::new("device.loop_attached", &[label]), true),
        Verdict::Mounted if others.is_empty() => (Note::new("device.still_mounted", &[label]), true),
        Verdict::Mounted => {
            let list = others.join(", ");
            let note = Note { key: "device.blocked", args: vec![label.into(), list], count: Some(others.len() as i64) };
            (note, true)
        }
        Verdict::Unknown => (Note::new("device.unconfirmed", &[label]), true),
    }
}

/// Whether a mounted row draws the release mark; an NFS share keeps its square.
pub fn releasable(entry: &RailEntry) -> bool {
    if !entry.mounted {
        return false;
    }
    match (entry.group, entry.kind) {
        (Group::Device, Kind::Volume) => entry.removable || entry.loop_device,
        (Group::Device, Kind::Phone) => true,
        (Group::Network, Kind::Share) if !entry.fstype.is_empty() => {
            matches!(entry.fstype.to_lowercase().as_str(), "cifs" | "smb2" | "smb3" | "smbfs")
        }
        (Group::Network, Kind::Share) => scheme_of(&entry.uri) != "nfs",
        _ => false,
    }
}

/// The one-click release, mirroring the rail menu rows.
pub fn release_action(entry: &RailEntry) -> Option<RailAction> {
    if !releasable(entry) {
        return None;
    }
    Some(match (entry.group, entry.kind) {
        (Group::Device, Kind::Phone) => RailAction::UnmountPhone,
        (Group::Device, _) => RailAction::Eject,
        _ => RailAction::Unmount,
    })
}

/// Eject by name first, else the row's unmount, never by position.
pub fn release_from_rows(rows: &[MenuRow]) -> Option<RailAction> {
    let actions: Vec<RailAction> = rows.iter().filter_map(MenuRow::action).collect();
    if actions.contains(&RailAction::Eject) {
        return Some(RailAction::Eject);
    }
    actions.into_iter().find(|a| matches!(a, RailAction::Unmount | RailAction::UnmountVolume | RailAction::UnmountPhone))
}

/// What Ctrl+E does: release a row by its key, or say why nothing was released.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Release {
    Hidden,
    Say(Note),
    Chosen(RailAction, String),
}

/// Ctrl+E from the rail cursor or from a listing inside a mount; a hidden rail hands off to a one-shot listing.
pub fn release(rail: Option<(&[RailEntry], Option<usize>)>, dir: &str, from_rail: bool, system_volume: bool, recent: bool) -> Release {
    let Some((entries, cursor)) = rail else { return Release::Hidden };
    let entry = if from_rail { cursor.and_then(|i| entries.get(i)) } else { holding(entries, dir) };
    if let (true, Some(e)) = (from_rail, entry) {
        if e.group == Group::Device && e.kind == Kind::Disk {
            return Release::Say(Note::new("device.system_drive", &[&e.label]));
        }
    }
    let Some(entry) = entry else {
        let key = if system_volume && !recent { "device.system_volume" } else { "device.no_volume" };
        return Release::Say(Note::new(key, &[]));
    };
    match release_from_rows(&rail_menu(entry)) {
        Some(pick) => Release::Chosen(pick, super::mounts::rail_key(entry)),
        None => Release::Say(Note::new("device.nothing_to_release", &[&entry.label])),
    }
}
