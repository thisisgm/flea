//! Local FUSE folders below Home supplement the Network listing without probing a mountpoint.
use crate::{
    backend::netfs,
    ui_logic_pl1::{Group, Kind, RailEntry},
};

pub fn rows(mountinfo: &str, home: &str, dropbox: &str) -> Vec<RailEntry> {
    let mut out = Vec::new();
    if dropbox.starts_with('/') && dropbox != "/" && !dropbox.contains('\0') {
        out.push(RailEntry {
            path: dropbox.into(),
            group: Group::Network,
            kind: Kind::Dropbox,
            mounted: true,
            ..RailEntry::default()
        });
    }
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return out;
    }
    for line in mountinfo.lines() {
        let Some((head, tail)) = line.split_once(" - ") else {
            continue;
        };
        let (Some(raw), Some(fstype)) = (
            head.split_whitespace().nth(4),
            tail.split_whitespace().next(),
        ) else {
            continue;
        };
        let lower = fstype.to_ascii_lowercase();
        if !(lower == "fuse" || lower.starts_with("fuse.")) || netfs::is_network_fstype(fstype) {
            continue;
        }
        let path = super::rail::unescape(raw);
        if !path
            .strip_prefix(home)
            .is_some_and(|rest| rest.starts_with('/'))
        {
            continue;
        }
        let row = RailEntry {
            label: super::mounts::leaf(&path),
            path,
            group: Group::Network,
            kind: Kind::Cloud,
            fstype: fstype.into(),
            mounted: true,
            glyph: "server".into(),
            ..RailEntry::default()
        };
        if let Some(old) = out
            .iter_mut()
            .find(|old| old.kind == Kind::Cloud && old.path == row.path)
        {
            *old = row;
        } else {
            out.push(row);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pb_network_2_cloud_mounts_keep_home_boundaries_escapes_and_stack_order() {
        let input = "1 0 0:1 / /home/u rw - fuse.mergerfs x rw\n2 0 0:2 / /home/u/My\\040Drive rw - fuse.mergerfs x rw\n3 0 0:3 / /home/u/My\\040Drive rw - fuse.other y rw\n4 0 0:4 / /home/user/Outside rw - fuse z rw\n5 0 0:5 / /home/u/Network rw - fuse.rclone n rw\n";
        let rows = rows(input, "/home/u/", "");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (&*rows[0].path, &*rows[0].label, &*rows[0].fstype),
            ("/home/u/My Drive", "My Drive", "fuse.other")
        );
        assert!(super::rows(input, "", "").is_empty());
    }
}
