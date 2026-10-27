//! The Network listing is built from GIO, saved places and mountinfo only, never a stat of a mountpoint.
use super::{
    mounts::{self, Place},
    protocols,
};
use crate::{
    backend::netfs,
    ui_logic_pl1::{Group, Kind, RailEntry},
};
use std::collections::HashSet;

pub fn rebuild(gio: &str, marks: &[Place], mountinfo: &str) -> Vec<RailEntry> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for mount in mounts::parse_mounts(gio) {
        let covered: Vec<_> = marks
            .iter()
            .filter(|p| covers(&mount.uri, &p.uri))
            .collect();
        if !covered.is_empty() {
            for place in covered {
                push(&mut out, &mut seen, &place.uri, &place.label, true);
            }
            continue;
        }
        let label = marks
            .iter()
            .find(|p| mounts::normalize(&p.uri) == mounts::normalize(&mount.uri))
            .map_or(&mount.label, |p| &p.label);
        push(&mut out, &mut seen, &mount.uri, label, true);
    }
    for place in marks {
        push(&mut out, &mut seen, &place.uri, &place.label, false);
    }
    let gio_kernel = mounts::gio_kernel_mounts(gio);
    let mut kernels: Vec<(String, String)> = Vec::new();
    for line in mountinfo.lines() {
        let Some((head, tail)) = line.split_once(" - ") else {
            continue;
        };
        let Some(raw) = head.split_whitespace().nth(4) else {
            continue;
        };
        let Some(fstype) = tail.split_whitespace().next() else {
            continue;
        };
        if !netfs::is_network_fstype(fstype) {
            continue;
        }
        let path = unescape(raw);
        if path == "/"
            || ["/run/", "/proc/", "/sys/", "/dev/"]
                .iter()
                .any(|prefix| path.starts_with(prefix))
            || ["/proc", "/sys", "/dev"].contains(&path.as_str())
        {
            continue;
        }
        if let Some(old) = kernels.iter_mut().find(|(p, _)| *p == path) {
            old.1 = fstype.into();
        } else {
            kernels.push((path, fstype.into()));
        }
    }
    for (path, fstype) in kernels {
        let lent = gio_kernel.iter().find(|row| row.path == path);
        let uri = lent.map_or_else(|| path.clone(), |row| row.uri.clone());
        let label = lent.map_or_else(|| mounts::leaf(&path), |row| row.label.clone());
        out.push(RailEntry {
            path,
            uri,
            label,
            fstype,
            group: Group::Network,
            kind: Kind::Share,
            mounted: true,
            glyph: "server".into(),
            editable: Some(false),
            ..RailEntry::default()
        });
    }
    out
}

fn push(
    out: &mut Vec<RailEntry>,
    seen: &mut HashSet<String>,
    uri: &str,
    label: &str,
    mounted: bool,
) {
    if !seen.insert(mounts::normalize(uri)) {
        return;
    }
    out.push(RailEntry {
        uri: uri.into(),
        label: label.into(),
        mounted,
        group: Group::Network,
        kind: Kind::Share,
        glyph: "server".into(),
        ..RailEntry::default()
    });
}

pub fn covers(live: &str, saved: &str) -> bool {
    let live = mounts::normalize(live);
    let saved = mounts::normalize(saved);
    matches!(
        protocols::scheme_of(&live).as_str(),
        "sftp" | "ftp" | "ftps" | "dav" | "davs"
    ) && super::bare_root(&live)
        && saved.len() > live.len()
        && saved.starts_with(&live)
}

pub fn release_uri(gio: &str, saved: &str) -> String {
    mounts::parse_mounts(gio)
        .into_iter()
        .find(|m| covers(&m.uri, saved))
        .map_or_else(|| saved.into(), |m| m.uri)
}

pub(super) fn unescape(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..index + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let n = (bytes[index + 1] - b'0') as u16 * 64
                + (bytes[index + 2] - b'0') as u16 * 8
                + (bytes[index + 3] - b'0') as u16;
            out.push(n as u8);
            index += 4;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
