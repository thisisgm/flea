//! Network mount rows, retaining CIFS ancestry so an automatic child is not another share.
use super::rail::unescape;
use crate::backend::netfs;
use std::collections::{HashMap, HashSet};

struct Mount {
    id: Option<u64>,
    parent: Option<u64>,
    root: String,
    path: String,
    fstype: String,
    source: String,
}

impl Mount {
    // Sample: "101 100 0:41 / /mnt/mac/Docs rw - cifs //mac/share/Docs rw".
    fn parse(line: &str) -> Option<Self> {
        let (head, tail) = line.split_once(" - ")?;
        let mut fields = head.split_whitespace();
        let id = fields.next()?.parse().ok();
        let parent = fields.next()?.parse().ok();
        fields.next()?;
        let root = unescape(fields.next()?);
        let path = unescape(fields.next()?);
        let mut tail = tail.split_whitespace();
        Some(Self {
            id,
            parent,
            root,
            path,
            fstype: tail.next()?.into(),
            source: unescape(tail.next().unwrap_or_default()),
        })
    }

    fn visible(&self) -> bool {
        netfs::is_network_fstype(&self.fstype)
            && self.path != "/"
            && !["/run/", "/proc/", "/sys/", "/dev/"]
                .iter()
                .any(|prefix| self.path.starts_with(prefix))
            && !["/proc", "/sys", "/dev"].contains(&self.path.as_str())
    }

    fn cifs(&self) -> bool {
        ["cifs", "smb2", "smb3", "smbfs"]
            .iter()
            .any(|kind| self.fstype.eq_ignore_ascii_case(kind))
    }

    fn continues(&self, parent: &Self) -> bool {
        if !self.cifs() || !parent.cifs() {
            return false;
        }
        let Some(relative) = self.path.strip_prefix(parent.path.trim_end_matches('/')) else {
            return false;
        };
        if !relative.starts_with('/') {
            return false;
        }
        let (Some(child), Some(parent)) = (self.remote_path(), parent.remote_path()) else {
            return false;
        };
        child == format!("{parent}{relative}")
    }

    // A non-root VFS root is relative to the UNC share; otherwise CIFS's source carries its prepath.
    // Sample: source "//mac/share", root "/Private" names "//mac/share/Private".
    fn remote_path(&self) -> Option<String> {
        let mut source = self.source.strip_prefix("//")?.splitn(3, '/');
        let host = source.next().filter(|part| !part.is_empty())?;
        let share = source.next().filter(|part| !part.is_empty())?;
        if !self.root.starts_with('/') {
            return None;
        }
        Some(if self.root == "/" {
            self.source.trim_end_matches('/').into()
        } else {
            format!("//{host}/{share}{}", self.root.trim_end_matches('/'))
        })
    }
}

pub(super) fn rows(body: &str) -> Vec<(String, String)> {
    let mounts: Vec<_> = body.lines().filter_map(Mount::parse).collect();
    let by_id: HashMap<_, _> = mounts
        .iter()
        .enumerate()
        .filter_map(|(index, mount)| mount.id.map(|id| (id, (index, mount))))
        .collect();
    let mut last = HashMap::new();
    for (index, mount) in mounts
        .iter()
        .enumerate()
        .filter(|(_, mount)| mount.visible())
    {
        last.insert(mount.path.as_str(), index);
    }
    let visible: HashSet<_> = last.values().copied().collect();
    let covered = |mount: &Mount| {
        let mut parent = mount.parent;
        // Mountinfo is a tree; a bounded walk also tolerates malformed fixture cycles.
        for _ in 0..mounts.len() {
            let Some((index, ancestor)) = parent.and_then(|id| by_id.get(&id).copied()) else {
                break;
            };
            if visible.contains(&index) && mount.continues(ancestor) {
                return true;
            }
            parent = ancestor.parent;
        }
        false
    };
    let mut emitted = HashSet::new();
    mounts
        .iter()
        .filter(|mount| mount.visible())
        .filter_map(|mount| {
            let index = *last.get(mount.path.as_str())?;
            let latest = &mounts[index];
            (emitted.insert(index) && !covered(latest))
                .then(|| (latest.path.clone(), latest.fstype.clone()))
        })
        .collect()
}
