//! The storage class of a folder a List view preview would read, from the mount table and sysfs alone.
//!
//! The folder's own mount decides, not its parent's: a mount point or a link onto another mount answers that
//! mount's class. Links are followed one component at a time, and a component on a mount that may stall is
//! classified from the table without being looked at.
use std::path::{Component, Path, PathBuf};

/// The storage classes the preview's load rules name, as the backend's extclass words them.
pub(super) const CLASSES: [&str; 4] = ["", "network", "phone", "usb"];

struct Mount {
    point: PathBuf,
    kind: String,
    /// The block device's major:minor, empty for a mount that names none.
    device: String,
}

/// The mount table's octal escapes (space, tab, newline, backslash) decoded.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let octal = bytes.get(at + 1..at + 4).filter(|digits| {
            bytes[at] == b'\\' && digits.iter().all(|digit| (b'0'..=b'7').contains(digit))
        });
        match octal {
            Some(digits) => {
                out.push(
                    digits
                        .iter()
                        .fold(0u8, |value, digit| value * 8 + (digit - b'0')),
                );
                at += 4;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The mount holding a path, from /proc/self/mountinfo: the deepest mount point it lies under.
fn holding(table: &str, path: &Path) -> Option<Mount> {
    table
        .lines()
        .filter_map(|line| {
            let (head, tail) = line.split_once(" - ")?;
            let mut fields = head.split_whitespace();
            let (device, point) = (fields.nth(2)?, fields.nth(1)?);
            let point = PathBuf::from(unescape(point));
            let kind = tail.split_whitespace().next()?.to_owned();
            path.starts_with(&point).then(|| Mount {
                point,
                kind,
                device: device.to_owned(),
            })
        })
        .max_by_key(|mount| mount.point.components().count())
}

/// A filesystem that may answer slowly: network, FUSE, or an automount not yet triggered.
fn slow(kind: &str) -> bool {
    flea::backend::netfs::can_wedge(kind) || kind == "autofs"
}

fn class_of(mount: &Mount, path: &Path) -> &'static str {
    let text = path.to_string_lossy();
    if slow(&mount.kind) {
        if text.contains("/gvfs/mtp:") || text.contains("/gvfs/gphoto2:") {
            "phone"
        } else {
            "network"
        }
    } else if flea::backend::blockdev::usb_for(Some(&mount.device)) {
        // The device decides, wherever it is mounted: sysfs is read, never the mount.
        "usb"
    } else {
        ""
    }
}

/// The class of the folder at `path` when it lies on another mount than `parent`, None when it shares the
/// parent's mount or cannot be walked (the read then reports why).
pub(super) fn crossing(table: &str, parent: &Path, path: &Path) -> Option<&'static str> {
    let home = holding(table, parent).map(|mount| mount.point);
    let differs = |mount: &Mount| home.as_ref() != Some(&mount.point);
    let mut pending: Vec<PathBuf> = path
        .components()
        .rev()
        .map(|part| PathBuf::from(part.as_os_str()))
        .collect();
    let mut resolved = PathBuf::from("/");
    let mut hops = 0;
    while let Some(part) = pending.pop() {
        match Path::new(&part).components().next()? {
            Component::RootDir => resolved = PathBuf::from("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                let candidate = resolved.join(name);
                // A slow mount is classified from the table, never looked at.
                if let Some(mount) = holding(table, &candidate).filter(|mount| slow(&mount.kind)) {
                    // The rest of the path still names it, as a phone's mtp: share below the gvfs mount.
                    let whole: PathBuf = std::iter::once(candidate)
                        .chain(pending.into_iter().rev())
                        .collect();
                    return differs(&mount).then(|| class_of(&mount, &whole));
                }
                let meta = std::fs::symlink_metadata(&candidate).ok()?;
                if !meta.file_type().is_symlink() {
                    resolved = candidate;
                    continue;
                }
                hops += 1;
                if hops > 40 {
                    return None;
                }
                let target = std::fs::read_link(&candidate).ok()?;
                pending.extend(
                    target
                        .components()
                        .rev()
                        .map(|part| PathBuf::from(part.as_os_str())),
                );
            }
            Component::Prefix(_) => return None,
        }
    }
    let mount = holding(table, &resolved)?;
    differs(&mount).then(|| class_of(&mount, &resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_s_own_mount_decides_its_class() {
        let dir = std::env::temp_dir().join(format!("flea-list-mount-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        for mount in ["disk", "share", "gvfs"] {
            std::fs::create_dir_all(dir.join(mount)).unwrap();
        }
        let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let link = |name: &str, to: String| std::os::unix::fs::symlink(to, dir.join(name)).unwrap();
        link("nas", at("share/docs"));
        link("phone", at("gvfs/mtp:host=Pixel/DCIM"));
        let line = |id: u32, device: &str, point: String, kind: &str| {
            format!("{id} 1 {device} / {point} rw - {kind} src rw\n")
        };
        let table = [
            line(21, "0:20", "/".into(), "btrfs"),
            line(57, "0:50", at("share"), "fuse.rclone"),
            line(58, "0:51", at("gvfs"), "fuse.gvfsd-fuse"),
            line(59, "0:999", at("disk"), "vfat"),
        ]
        .concat();
        let class = |name: &str| crossing(&table, &dir, &dir.join(name));
        // A folder on its parent's mount answers nothing; the read follows the parent's own rules.
        assert_eq!(class("plain"), None);
        // Links onto a share or a phone answer that storage without touching it; a local mount point its own.
        assert_eq!(class("nas"), Some("network"));
        assert_eq!(class("phone"), Some("phone"));
        assert_eq!(class("disk"), Some(""));
        // Under /media or not, a disk sysfs does not call USB or removable is local.
        let internal = Mount {
            point: "/media/data".into(),
            kind: "ext4".into(),
            device: "0:999".into(),
        };
        assert_eq!(class_of(&internal, Path::new("/media/data/photos")), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
