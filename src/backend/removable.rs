// Whether a mount sits on a pull-out disk (usb, SD, Thunderbolt or USB4, btrfs on one, a dm stack); extclass's class stays narrower.
use crate::backend::mountinfo::MountEntry;
use std::path::{Path, PathBuf};

// dm-crypt over LVM over a partition is three; anything deeper than this is not a stack a removable drive carries.
const MAX_STACK: usize = 8;

// Sample input: majmin "0:29" with source "/dev/sdb1" reads the disk through sys/class/block/sdb1, as btrfs names no block device.
pub fn removable_under(sys: &Path, dev: &Path, entry: &MountEntry) -> bool {
    let by_number = block_number(&entry.majmin)
        .and_then(|majmin| std::fs::canonicalize(sys.join("dev/block").join(majmin)).ok());
    match by_number.or_else(|| source_device(sys, dev, &entry.source)) {
        Some(device) => device_is_removable(sys, &device, 0),
        None => false,
    }
}

// Sample input: "8:17" answers itself; major 0 ("0:29", btrfs, overlay, tmpfs) is anonymous and never in /sys/dev/block.
fn block_number(majmin: &str) -> Option<&str> {
    let (major, minor) = majmin.split_once(':')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (digits(major) && digits(minor) && major != "0").then_some(majmin)
}

// Sample input: "/dev/mapper/luks-1" resolves to dev/dm-0 and answers sys/class/block/dm-0 canonicalized; "//nas/share" answers None.
fn source_device(sys: &Path, dev: &Path, source: &str) -> Option<PathBuf> {
    let rest = source
        .strip_prefix("/dev/")
        .filter(|rest| !rest.is_empty())?;
    let node = std::fs::canonicalize(dev.join(rest)).ok()?;
    let name = node.file_name()?;
    std::fs::canonicalize(sys.join("class/block").join(name)).ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

// A device directory under sys/devices: its partition's disk, a dm stack's members, then every ancestor's removable attribute.
fn device_is_removable(sys: &Path, device: &Path, depth: usize) -> bool {
    if crate::backend::extclass::link_is_usb(&device.to_string_lossy()) {
        return true;
    }
    let disk = if device.join("partition").is_file() {
        device.parent().unwrap_or(device)
    } else {
        device
    };
    if read_trimmed(&disk.join("removable")).as_deref() == Some("1") {
        return true;
    }
    if is_sd_card(disk) {
        return true;
    }
    if depth < MAX_STACK {
        if let Ok(members) = std::fs::read_dir(disk.join("slaves")) {
            let mut members = members
                .flatten()
                .filter_map(|m| std::fs::canonicalize(m.path()).ok());
            if members.any(|m| device_is_removable(sys, &m, depth + 1)) {
                return true;
            }
        }
    }
    removable_ancestor(sys, disk)
}

// An SD card answers "SD" in its card's type, so an SD reader on PCI counts while soldered eMMC ("MMC") does not.
fn is_sd_card(disk: &Path) -> bool {
    let name = disk
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.starts_with("mmcblk")
        && read_trimmed(&disk.join("device/type")).is_some_and(|t| t == "SD" || t == "SD-combo")
}

// The driver core marks a bus device below an external-facing port "removable"; a disk's own file holds only "0" or "1".
fn removable_ancestor(sys: &Path, disk: &Path) -> bool {
    // Canonical like the disk path it bounds, so a sysfs root reached through a link still ends the walk at devices.
    let Ok(top) = std::fs::canonicalize(sys.join("devices")) else {
        return false;
    };
    disk.ancestors()
        .skip(1)
        .take_while(|a| a.starts_with(&top) && *a != top)
        .any(|a| read_trimmed(&a.join("removable")).as_deref() == Some("removable"))
}

#[cfg(test)]
#[path = "removable_tests.rs"]
mod tests;
