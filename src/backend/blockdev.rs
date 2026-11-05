// Whether a block device is a USB or removable disk, from sysfs alone: no mount is looked at.
// Shared by extclass and the GUI's folder preview, which classifies a mount point it may not read.
use std::path::Path;

// Sample sysfs target: "../../devices/pci0000:00/0000:00:14.0/usb2/2-1/host0/block/sdb/sdb1" trues.
pub fn link_is_usb(target: &str) -> bool {
    target.split('/').any(|seg| seg == "usb" || usb_bus(seg))
}

// A bus segment is "usb" plus its number, so a block device literally named "usbstick" never matches.
fn usb_bus(seg: &str) -> bool {
    seg.len() > 3 && seg.starts_with("usb") && seg.as_bytes()[3..].iter().all(|b| b.is_ascii_digit())
}

// Sample: removable "1\n" trues with no link at all.
pub fn is_usb(link_target: Option<&str>, removable: Option<&str>) -> bool {
    if removable.is_some_and(|v| v.trim() == "1") {
        return true;
    }
    link_target.is_some_and(link_is_usb)
}

// Sample target: "../../devices/pci/host0/block/sdb/sdb1" answers Some("sdb").
pub fn disk_of_link(target: &str) -> Option<&str> {
    let parts: Vec<&str> = target.split('/').collect();
    let disk = parts.iter().position(|seg| *seg == "block").and_then(|at| parts.get(at + 1))?;
    if disk.is_empty() || *disk == "." || *disk == ".." {
        return None;
    }
    Some(disk)
}

// A mount that names no block device answers false without touching sysfs.
pub fn usb_for(majmin: Option<&str>) -> bool {
    let majmin = match majmin {
        Some(m) if !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit() || b == b':') => m,
        _ => return false,
    };
    let target =
        std::fs::read_link(Path::new("/sys/dev/block").join(majmin)).map(|p| p.to_string_lossy().into_owned()).ok();
    let removable = target
        .as_deref()
        .and_then(disk_of_link)
        .and_then(|disk| std::fs::read_to_string(Path::new("/sys/block").join(disk).join("removable")).ok());
    is_usb(target.as_deref(), removable.as_deref())
}
