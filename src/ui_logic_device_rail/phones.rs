//! Phones and cameras from gio mount -li: the MTP, AFC and GPhoto2 volumes lsblk never lists.
use super::{Group, Kind, RailEntry};

#[derive(Default)]
struct Block {
    label: String,
    uri: String,
    uuid: String,
    in_block: bool,
    monitor: &'static str,
    can_mount: bool,
}

/// One row per phone: column-zero Volume blocks on the three gvfs monitors, an iPhone's two monitors folded into one.
pub fn parse_phones(output: &str) -> Vec<RailEntry> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut mounted: Vec<String> = Vec::new();
    let mut open = false;
    for line in output.split('\n') {
        if let Some(label) = volume_head(line) {
            blocks.push(Block { label, ..Block::default() });
            open = true;
            continue;
        }
        if let Some(uri) = column_zero_mount(line) {
            mounted.push(uri);
        }
        if !line.starts_with(char::is_whitespace) {
            open = false;
            continue;
        }
        let Some(v) = blocks.last_mut().filter(|_| open) else { continue };
        let body = line.trim();
        if let Some(monitor) = body.strip_prefix("Type: GProxyVolume (GProxyVolumeMonitor").and_then(|m| m.strip_suffix(')')) {
            v.monitor = match monitor {
                "MTP" => "MTP",
                "GPhoto2" => "GPhoto2",
                "Afc" => "Afc",
                _ => v.monitor,
            };
        }
        if let Some(value) = single_value(body, "uuid=") {
            v.uuid = value;
        }
        if let Some(value) = single_value(body, "activation_root=") {
            v.uri = value;
        }
        if body == "can_mount=1" {
            v.can_mount = true;
        }
        if mount_index(body).is_some() {
            v.in_block = true;
        }
    }
    for b in blocks.iter_mut().filter(|b| b.monitor == "Afc") {
        b.uri = if b.uuid.is_empty() { String::new() } else { format!("afc://{}/", b.uuid) };
    }
    let mut kept: Vec<(Block, bool)> = Vec::new();
    let mut serials: Vec<String> = Vec::new();
    for b in blocks {
        let root = mounted.contains(&b.uri);
        let is_mounted = if b.monitor == "Afc" { root } else { b.in_block || root };
        let live = b.monitor == "Afc" || b.can_mount || is_mounted;
        if b.monitor.is_empty() || b.uri.is_empty() || !live || kept.iter().any(|(k, _)| k.uri == b.uri) {
            continue;
        }
        if b.monitor == "Afc" {
            serials.push(b.uuid.replace('-', "").to_uppercase());
        }
        kept.push((b, is_mounted));
    }
    kept.into_iter()
        .filter(|(b, _)| !(b.monitor == "GPhoto2" && serials.iter().any(|s| b.uri.to_uppercase().contains(s.as_str()))))
        .map(|(b, is_mounted)| entry(b, is_mounted))
        .collect()
}

fn volume_head(line: &str) -> Option<String> {
    let rest = line.strip_prefix("Volume(")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let label = rest[digits..].strip_prefix("):").filter(|_| digits > 0)?.trim();
    (!label.is_empty()).then(|| label.to_string())
}

fn mount_index(body: &str) -> Option<&str> {
    let rest = body.strip_prefix("Mount(")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[digits..].strip_prefix("):").filter(|_| digits > 0)
}

fn column_zero_mount(line: &str) -> Option<String> {
    let rest = mount_index(line)?;
    let at = rest.match_indices("->").map(|(at, _)| at).find(|&at| {
        let name = rest[..at].trim();
        let mut tail = rest[at + 2..].split_whitespace();
        !name.is_empty() && tail.next().is_some() && tail.next().is_none()
    })?;
    rest[at + 2..].split_whitespace().next().map(str::to_string)
}

fn single_value(body: &str, key: &str) -> Option<String> {
    let value = body.strip_prefix(key)?;
    (!value.is_empty() && !value.contains(char::is_whitespace)).then(|| value.to_string())
}

fn entry(b: Block, mounted: bool) -> RailEntry {
    let label = if b.monitor == "Afc" { b.label.strip_prefix("Documents on ").unwrap_or(&b.label).to_string() } else { b.label };
    RailEntry {
        label,
        group: Group::Device,
        kind: Kind::Phone,
        uri: b.uri,
        mounted,
        glyph: if b.monitor == "GPhoto2" { "camera" } else { "smartphone" }.into(),
        ..RailEntry::default()
    }
}
