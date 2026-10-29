//! The DEVICES half of the rail: one lsblk listing turned into disk and volume rows.
use super::lsblk::{bytes, children, device_path, has, is_true, mount_of, mountpoints, parse_hex, parse_nodes, raw_name, text};
use super::{Group, Kind, RailEntry};
use crate::jsondoc::Json;

/// The lsblk call whose columns every reader here expects.
pub const LSBLK_ARGV: &[&str] = &[
    "lsblk",
    "--bytes",
    "--json",
    "-o",
    "NAME,PATH,LABEL,MOUNTPOINTS,RM,RO,TRAN,SIZE,TYPE,MODEL,FSTYPE,FSVER,PARTTYPENAME,PARTTYPE,PTTYPE,PARTN,UUID",
];

/// One disk row for the disk carrying /, then a volume row per browsable volume; an unparsable body is no rows.
pub fn parse_devices(body: &str, unmounted: bool) -> Vec<RailEntry> {
    let Some(nodes) = parse_nodes(body) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(system) = nodes.iter().find(|n| has(n, "name") && holds_root(n)) {
        out.push(RailEntry {
            kind: Kind::Disk,
            group: Group::Device,
            label: raw_name(system),
            device: device_path(system),
            path: "/".into(),
            mounted: true,
            size: bytes(system),
            glyph: "drive".into(),
            ..RailEntry::default()
        });
    }
    for node in &nodes {
        if !has(node, "name") || is_pseudo(node) {
            continue;
        }
        let walk = Walk { unmounted, removable_disk: unpluggable(node), holder: "", under_loop: false };
        collect_volumes(std::slice::from_ref(node), "", false, &mut out, &walk);
    }
    dedupe(out)
}

/// zram stays out; a loop device counts only while it or a child is mounted.
pub fn is_pseudo(node: &Json) -> bool {
    let name = text(node, "name");
    name.starts_with("zram") || (name.starts_with("loop") && !holds_mount(node))
}

fn holds_mount(node: &Json) -> bool {
    !mount_of(node).is_empty() || children(node).iter().any(holds_mount)
}

/// Whether / is mounted on this node or anything under it.
pub fn holds_root(node: &Json) -> bool {
    mountpoints(node).iter().any(|p| p == "/") || children(node).iter().any(holds_root)
}

struct Walk<'a> {
    unmounted: bool,
    removable_disk: bool,
    holder: &'a str,
    under_loop: bool,
}

fn collect_volumes(nodes: &[Json], model: &str, unplugs: bool, out: &mut Vec<RailEntry>, walk: &Walk) {
    for n in nodes {
        let kids = children(n);
        let own = if has(n, "model") { text(n, "model") } else { model.to_string() };
        let pulls = unpluggable(n) || (unplugs && text(n, "type") == "part");
        let loop_here = walk.under_loop || text(n, "name").starts_with("loop");
        let mounted = !mount_of(n).is_empty();
        let windows_data = text(n, "fstype").to_lowercase() == "ntfs";
        let wanted = pulls || mounted || ((walk.unmounted || windows_data) && browsable(n) && !loop_here) || is_locked_node(n);
        if has(n, "name") && (kids.is_empty() || mounted) && !hidden(n) && !holds_root(n) && wanted {
            out.push(volume_row(n, &own, pulls, walk.removable_disk, walk.holder, loop_here));
        }
        let holder = container_name(n);
        let next = Walk {
            unmounted: walk.unmounted,
            removable_disk: walk.removable_disk || unpluggable(n),
            holder: &holder,
            under_loop: loop_here,
        };
        collect_volumes(kids, &own, pulls, out, &next);
    }
}

/// A leaf udisks hides is no row in any branch: no filesystem, a member, firmware, recovery or system-only.
pub fn hidden(n: &Json) -> bool {
    let fs = text(n, "fstype").to_lowercase();
    if mount_of(n).is_empty() && fs.is_empty() {
        return true;
    }
    if fs == "swap" || fs.ends_with("_member") || fs == "bcache" || fs == "ceph" || fs == "drbd" {
        return true;
    }
    if fs.starts_with("crypto_") {
        return false;
    }
    (is_firmware_part(n) && !is_isohybrid_keep(n)) || is_recovery_label(n) || is_system_mount_only(n)
}

fn is_firmware_part(n: &Json) -> bool {
    let lower = text(n, "parttype").to_lowercase();
    if lower.is_empty() {
        return false;
    }
    if lower.contains('-') {
        return GPT_HIDDEN.contains(&lower.as_str());
    }
    parse_hex(&lower).is_some_and(|num| MBR_HIDDEN.contains(&num))
}

const GPT_HIDDEN: &[&str] = &[
    "c12a7328-f81f-11d2-ba4b-00a0c93ec93b",
    "21686148-6449-6e6f-744e-656564454649",
    "e3c9e316-0b5c-4db8-817d-f92df00215ae",
    "de94bba4-06d1-4d40-a16a-bfd50179d6ac",
    "bc13c2ff-59e6-4262-a352-b275fd6f7172",
    "426f6f74-0000-11aa-aa11-00306543ecac",
    "a19d880f-05fc-4d3b-a006-743f0f84911e",
    "e6d6d379-f507-44c2-a23c-238f2a3df928",
];

const MBR_HIDDEN: &[i64] = &[
    0x0, 0x5, 0xf, 0x85, 0x11, 0x12, 0x14, 0x16, 0x17, 0x1b, 0x1c, 0x1e, 0x27, 0x3d, 0x84, 0x8d, 0x8e, 0x90, 0x91,
    0x92, 0x93, 0x97, 0x98, 0x9a, 0x9b, 0xab, 0xbb, 0xc2, 0xc3, 0xdd, 0xef, 0xfd,
];

fn is_isohybrid_keep(n: &Json) -> bool {
    if text(n, "pttype").to_lowercase() != "dos" {
        return false;
    }
    if !matches!(parse_hex(&text(n, "parttype")), Some(0x0 | 0x17)) {
        return false;
    }
    let partn = match n.get("partn") {
        Some(Json::Num(v)) | Some(Json::Str(v)) => v.clone(),
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    };
    let fs = text(n, "fstype").to_lowercase();
    (partn == "1" || partn == "2") && (fs == "iso9660" || fs == "udf")
}

const RECOVERY_LABELS: &[&str] = &[
    "Recovery", "RECOVERY", "Lenovo_Recovery", "HP_RECOVERY", "Recovery_Partition", "DellUtility", "DellRestore",
    "IBM_SERVICE", "SERVICEV001", "SERVICEV002", "SYSTEM_RESERVED", "System_Reserved", "WINRE_DRV", "DIAGS", "IntelRST",
];

fn is_recovery_label(n: &Json) -> bool {
    let fs = text(n, "fstype").to_lowercase();
    let label = text(n, "label").replace(' ', "_");
    (fs == "ntfs" || fs == "vfat") && RECOVERY_LABELS.contains(&label.as_str())
}

/// Every real mountpoint is a system path, and there is at least one.
pub fn is_system_mount_only(n: &Json) -> bool {
    let points: Vec<String> = mountpoints(n).into_iter().filter(|p| p.starts_with('/')).collect();
    !points.is_empty() && points.iter().all(|p| is_system_path(p))
}

const SYSTEM_EXACT: &[&str] = &[
    "/bin", "/boot", "/compat/linux/proc", "/compat/linux/sys", "/etc", "/home", "/lib", "/lib64", "/libexec",
    "/live/cow", "/live/image", "/media", "/mnt", "/net", "/opt", "/rescue", "/root", "/run", "/sbin", "/srv", "/tmp",
    "/usr", "/usr/X11R6", "/usr/local", "/usr/obj", "/usr/ports", "/usr/src", "/usr/xobj", "/var", "/var/crash",
    "/var/local", "/var/log", "/var/log/audit", "/var/mail", "/var/run", "/var/tmp",
];

/// glib's system path list: exact names, a hidden segment, or anything under /dev, /proc and /sys.
pub fn is_system_path(p: &str) -> bool {
    if p == "/" || p.contains("/.") || p == "/dev" || p == "/proc" || p == "/sys" {
        return true;
    }
    p.starts_with("/dev/") || p.starts_with("/proc/") || p.starts_with("/sys/") || SYSTEM_EXACT.contains(&p)
}

/// One row per device path and per btrfs uuid; a mounted duplicate replaces an unmounted one in place.
pub fn dedupe(rows: Vec<RailEntry>) -> Vec<RailEntry> {
    let mut head: Vec<RailEntry> = Vec::new();
    let mut at: Vec<(String, usize)> = Vec::new();
    let mut by_uuid: Vec<(String, usize)> = Vec::new();
    let find = |list: &Vec<(String, usize)>, key: &str| list.iter().find(|(k, _)| k == key).map(|(_, i)| *i);
    let set = |list: &mut Vec<(String, usize)>, key: &str, index: usize| match list.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = index,
        None => list.push((key.to_string(), index)),
    };
    for row in rows {
        let dkey = row.device.clone();
        let ukey = row.uuid.clone();
        let dup = if find(&at, &dkey).is_some() && !dkey.is_empty() {
            find(&at, &dkey)
        } else if !ukey.is_empty() {
            find(&by_uuid, &ukey)
        } else {
            None
        };
        match dup {
            None => {
                set(&mut at, &dkey, head.len());
                if !ukey.is_empty() {
                    set(&mut by_uuid, &ukey, head.len());
                }
                head.push(row);
            }
            Some(index) if row.mounted && !head[index].mounted => {
                set(&mut at, &dkey, index);
                if !ukey.is_empty() {
                    set(&mut by_uuid, &ukey, index);
                }
                head[index] = row;
            }
            Some(_) => {}
        }
    }
    head
}

/// A crypto container leaf with nothing mounted on it.
pub fn is_locked_node(n: &Json) -> bool {
    children(n).is_empty() && mount_of(n).is_empty() && text(n, "fstype").to_lowercase().starts_with("crypto_")
}

fn browsable(n: &Json) -> bool {
    !text(n, "fstype").to_lowercase().starts_with("crypto_") && !text(n, "parttypename").to_lowercase().contains("efi")
}

/// A drive somebody can pull out: removable media or a USB transport.
pub fn unpluggable(n: &Json) -> bool {
    is_true(n, "rm") || text(n, "tran").to_lowercase() == "usb"
}

/// The mount wait starts when gio exits 0, never at launch, so a slow polkit prompt is not a timeout.
pub fn mount_timer_start(exit_code: i32) -> bool {
    exit_code == 0
}

/// The name a crypto container's owner gave it, "" for any other node.
pub fn container_name(n: &Json) -> String {
    if text(n, "fstype").to_lowercase().starts_with("crypto_") && has(n, "label") {
        return text(n, "label");
    }
    String::new()
}

fn volume_row(n: &Json, model: &str, unplugs: bool, removable_disk: bool, holder: &str, loop_device: bool) -> RailEntry {
    let path = mount_of(n);
    let crypt = text(n, "type") == "crypt";
    let label = if crypt && !holder.is_empty() {
        holder.to_string()
    } else if has(n, "label") {
        text(n, "label")
    } else if !model.is_empty() {
        model.to_string()
    } else {
        raw_name(n)
    };
    let fstype = text(n, "fstype");
    let uuid = if fstype.to_lowercase() == "btrfs" && has(n, "uuid") { text(n, "uuid") } else { String::new() };
    RailEntry {
        kind: Kind::Volume,
        group: Group::Device,
        label,
        device: device_path(n),
        mounted: !path.is_empty(),
        path,
        removable: unplugs || (crypt && removable_disk),
        media_removable: is_true(n, "rm"),
        read_only: is_true(n, "ro"),
        size: bytes(n),
        locked: is_locked_node(n),
        fsver: text(n, "fsver"),
        fstype,
        uuid,
        loop_device,
        glyph: if is_locked_node(n) { "lock" } else { "drive" }.into(),
        ..RailEntry::default()
    }
}

/// The machine's row reads the hostname alone; the kernel name stays for a box with none.
pub fn host_label(fallback: &str, hostname_file: &str) -> String {
    let host = hostname_file.trim();
    if host.is_empty() { fallback.to_string() } else { host.to_string() }
}
