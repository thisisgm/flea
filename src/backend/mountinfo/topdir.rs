use super::*;

// Every mount point and its filesystem type, in file order, read once for a caller that asks about many paths.
pub(crate) fn mounts_in(body: &str) -> Vec<(PathBuf, String)> {
    body.lines()
        .filter_map(parse_line)
        .map(|(mount, fstype, _, _)| (mount, fstype))
        .collect()
}

// Each mount's point, its root within the filesystem, and whether a topdir Trash serves it: a block-backed subvolume or bind mount, so a probe never opens a network, FUSE or autofs mount.
// Sample: `28 22 259:1 /@data /mnt/data rw - btrfs /dev/nvme0n1p1 rw` yields ("/mnt/data", "/@data", true); `40 22 0:50 / /net rw - nfs4 srv:/x rw` and `50 22 0:60 /sub /mnt/win rw - fuseblk /dev/sda1 rw` are not served.
pub(crate) fn mount_roots_in(body: &str) -> Vec<(PathBuf, PathBuf, bool)> {
    let mut out = Vec::new();
    for line in body.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(split) = fields.iter().position(|field| *field == "-") else {
            continue;
        };
        if fields.len() <= split + 2 || fields.len() < 6 {
            continue;
        }
        let root = PathBuf::from(OsString::from_vec(unescape(fields[3])));
        let served = root != Path::new("/")
            && fields[split + 2].starts_with("/dev/")
            && !fields[split + 1].starts_with("fuse");
        let mount = PathBuf::from(OsString::from_vec(unescape(fields[4])));
        out.push((mount, root, served));
    }
    out
}

// The deepest mount holding path, the later line winning a tie as the kernel's own stacking order does.
pub(crate) fn deepest<'a>(
    path: &Path,
    mounts: &'a [(PathBuf, PathBuf, bool)],
) -> Option<&'a (PathBuf, PathBuf, bool)> {
    let mut best: Option<(usize, &(PathBuf, PathBuf, bool))> = None;
    for entry in mounts.iter().filter(|entry| path.starts_with(&entry.0)) {
        let depth = entry.0.components().count();
        if best.map(|(old, _)| depth >= old).unwrap_or(true) {
            best = Some((depth, entry));
        }
    }
    best.map(|(_, entry)| entry)
}
