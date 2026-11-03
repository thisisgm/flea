// Fake /sys/block trees shared by the flasher's tests: a USB stick, an NVMe system disk and their links.
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};

// The relative link text a disk's device entry really holds: the SCSI address three levels up, naming no bus.
pub(crate) const DEVICE_LINK: &str = "../../../2:0:0:0";

// The swaps file with no active swap, the header line alone.
pub(crate) const NO_SWAPS: &str = "Filename Type Size Used Priority\n";

pub(crate) struct Fake {
    pub(crate) _dir: TestDir,
    pub(crate) sys: PathBuf,
    pub(crate) dev: PathBuf,
}

pub(crate) fn fake(tag: &str) -> Fake {
    let dir = TestDir::new(tag);
    let sys = dir.dir("sys");
    std::fs::create_dir_all(sys.join("block")).unwrap();
    std::fs::create_dir_all(sys.join("dev/block")).unwrap();
    let dev = dir.dir("dev");
    Fake { _dir: dir, sys, dev }
}

// Sample by-label tree: dev/disk/by-label/128GB answers the sda1 it points at.
pub(crate) fn label(dev: &Path, name: &str, part: &str) {
    let dir = dev.join("disk/by-label");
    std::fs::create_dir_all(&dir).unwrap();
    let _ = std::os::unix::fs::symlink(part, dir.join(name));
}

// Sample bus: "/usb2/2-1" answers sys/block/sda linking to sys/devices/usb2/2-1/block/sda, the kernel's own shape.
pub(crate) fn disk(sys: &Path, dev: &str, removable: &str, ro: &str, bytes: u64, seq: &str, bus: &str) {
    let rel = format!("devices/{}/block/{dev}", bus.trim_start_matches('/'));
    let base = sys.join(&rel);
    std::fs::create_dir_all(base.join("holders")).unwrap();
    std::fs::write(base.join("removable"), removable).unwrap();
    std::fs::write(base.join("ro"), ro).unwrap();
    std::fs::write(base.join("size"), format!("{}", bytes / 512)).unwrap();
    std::fs::write(base.join("diskseq"), seq).unwrap();
    let _ = std::os::unix::fs::symlink(DEVICE_LINK, base.join("device"));
    let _ = std::os::unix::fs::symlink(format!("../{rel}"), sys.join("block").join(dev));
}

pub(crate) fn identity(sys: &Path, dev: &str, vendor: &str, model: &str, serial: &str) {
    let base = sys.join("block").join(dev);
    std::fs::write(base.join("vendor"), vendor).unwrap();
    std::fs::write(base.join("model"), model).unwrap();
    if !serial.is_empty() {
        std::fs::write(base.join("serial"), serial).unwrap();
    }
}

// Sample link: sys/dev/block/8:17 answers ../../block/sda/sda1, the shape read_link returns.
pub(crate) fn part(sys: &Path, dev: &str, part: &str, majmin: &str) {
    let base = sys.join("block").join(dev).join(part);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("dev"), majmin).unwrap();
    std::fs::write(base.join("partition"), "1").unwrap();
    let _ = std::os::unix::fs::symlink(format!("../../block/{dev}/{part}"), sys.join("dev/block").join(majmin));
}

pub(crate) fn devlink(sys: &Path, majmin: &str, target: &str) {
    let _ = std::os::unix::fs::symlink(target, sys.join("dev/block").join(majmin));
}

// Sample chain: dm-0 built on nvme0n1p2, so slaves/nvme0n1p2 exists and dev/mapper/<name> links to dm-0.
pub(crate) fn mapped(f: &Fake, name: &str, kernel: &str, majmin: &str, slaves: &[&str]) {
    let base = f.sys.join("block").join(kernel);
    std::fs::create_dir_all(base.join("holders")).unwrap();
    std::fs::create_dir_all(base.join("slaves")).unwrap();
    for slave in slaves {
        std::fs::write(base.join("slaves").join(slave), "").unwrap();
    }
    std::fs::create_dir_all(f.dev.join("mapper")).unwrap();
    let _ = std::os::unix::fs::symlink(format!("../{kernel}"), f.dev.join("mapper").join(name));
    if !majmin.is_empty() {
        devlink(&f.sys, majmin, &format!("../../block/{kernel}"));
    }
}

pub(crate) fn roots() -> (PathBuf, PathBuf) {
    (PathBuf::new(), PathBuf::new())
}

// Sample mountinfo: root on nvme0n1p1, the box's own shape with one disk swapped in.
pub(crate) fn root_mountinfo() -> String {
    "28 1 8:1 / / rw - ext4 /dev/nvme0n1p1 rw\n".to_string()
}

pub(crate) fn stick(sys: &Path) {
    disk(sys, "sda", "1\n", "0\n", 124_623_257_600, "2\n", "/usb2/1-1");
    identity(sys, "sda", "General", "General USB Flash Disk", "2003007157171692");
    part(sys, "sda", "sda1", "8:17");
}

pub(crate) fn system(sys: &Path) {
    disk(sys, "nvme0n1", "0\n", "0\n", 512_000_000_000, "9\n", "/pci0000:00/nvme");
    identity(sys, "nvme0n1", "Samsung", "Samsung SSD 970", "");
    part(sys, "nvme0n1", "nvme0n1p1", "8:1");
    devlink(sys, "8:1", "../../block/nvme0n1/nvme0n1p1");
}


// The marker and name prefix a direct root carries, the same guard TestDir applies to its own.
const DIRECT_MARKER: &str = ".flea-test-sandbox";
const DIRECT_PREFIX: &str = "flea-test-direct-";
// The flag the code under test checks, so the fixture opens targets the same way.
pub(crate) use crate::backend::flashio::O_DIRECT;

static NEXT_DIRECT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// A marked root inside this worktree, on the filesystem that enforces O_DIRECT alignment; /tmp may be tmpfs, which does not.
pub(crate) struct DirectDir {
    path: PathBuf,
}

impl DirectDir {
    // None, with a note on stderr, where the filesystem cannot report usable direct-I/O alignment.
    pub(crate) fn new(tag: &str) -> Option<DirectDir> {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/tmp");
        std::fs::create_dir_all(&base).expect("the direct root's parent in the worktree");
        let n = NEXT_DIRECT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = base.join(format!("{DIRECT_PREFIX}{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir(&path).expect("a fresh direct root");
        std::fs::write(path.join(DIRECT_MARKER), "flea test sandbox\n").expect("the direct root's marker");
        let dir = DirectDir { path };
        if !dir.enforces_alignment() {
            eprintln!("flea test: {} does not report usable O_DIRECT alignment, so this test proves nothing here", base.display());
            return None;
        }
        Some(dir)
    }

    // Azure ARM can hang on a deliberately misaligned write, so query the kernel's capability instead.
    fn enforces_alignment(&self) -> bool {
        use std::os::unix::fs::OpenOptionsExt;
        let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).custom_flags(O_DIRECT).open(self.path.join("probe")) else { return false };
        // SAFETY: libc::statx is an integer-only C struct, so zero is a valid initial value.
        let mut info = unsafe { std::mem::zeroed::<libc::statx>() };
        // SAFETY: the descriptor is live, the path is NUL-terminated and the output has the native layout.
        let result = unsafe {
            libc::statx(
                std::os::fd::AsRawFd::as_raw_fd(&file),
                c"".as_ptr(),
                libc::AT_EMPTY_PATH,
                libc::STATX_DIOALIGN,
                &mut info,
            )
        };
        reported_alignment(result, &info)
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    // The drive stand-in: a regular file of this size, opened read-write with O_DIRECT like the helper's descriptor.
    pub(crate) fn direct_target(&self, name: &str, bytes: u64) -> std::fs::File {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).custom_flags(O_DIRECT).open(self.join(name)).expect("an O_DIRECT target");
        file.set_len(bytes).expect("a sized target");
        file
    }
}

fn reported_alignment(result: libc::c_int, info: &libc::statx) -> bool {
    result == 0 && info.stx_mask & libc::STATX_DIOALIGN != 0 && info.stx_dio_mem_align > 1
}

#[test]
fn statx_layout_matches_linux_direct_io_contract() {
    assert_eq!(std::mem::size_of::<libc::statx>(), 256);
    assert_eq!(std::mem::offset_of!(libc::statx, stx_mask), 0);
    assert_eq!(std::mem::offset_of!(libc::statx, stx_dio_mem_align), 152);
    assert_eq!(std::mem::offset_of!(libc::statx, stx_dio_offset_align), 156);
    assert_eq!(libc::STATX_DIOALIGN, 0x2000);
    assert_eq!(libc::AT_EMPTY_PATH, 0x1000);
}

#[test]
fn statx_alignment_fails_closed_without_a_successful_usable_capability() {
    for (result, mask, alignment, expected) in [
        (0, libc::STATX_DIOALIGN, 64, true),
        (0, libc::STATX_DIOALIGN, 4096, true),
        (0, libc::STATX_DIOALIGN, 2, true),
        (0, libc::STATX_DIOALIGN | libc::STATX_TYPE, 64, true),
        (0, 0, 4096, false),
        (0, libc::STATX_DIOALIGN, 0, false),
        (0, libc::STATX_DIOALIGN, 1, false),
        (-1, libc::STATX_DIOALIGN, 4096, false),
    ] {
        // SAFETY: the test writes only integer fields of the zero-initialized native C struct.
        let mut info = unsafe { std::mem::zeroed::<libc::statx>() };
        info.stx_mask = mask;
        info.stx_dio_mem_align = alignment;
        assert_eq!(
            reported_alignment(result, &info),
            expected,
            "result={result}, mask={mask:#x}, alignment={alignment}"
        );
    }
}

impl Drop for DirectDir {
    fn drop(&mut self) {
        let owned = self.path.is_absolute() && self.path.join(DIRECT_MARKER).is_file()
            && self.path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(DIRECT_PREFIX));
        if owned {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
