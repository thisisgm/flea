// Durable copies onto usb, phone and network targets: fsync each file, then each directory.
#[cfg(test)]
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::sync::Condvar;

// Test builds only: forced-durable paths, injected mountinfo text, fsync fault flags, flush counts; a release build has none of it.
#[cfg(test)]
thread_local! {
    static FORCE: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    // Sample input: Some("... vfat /dev/sda1 ...") batches a marked dest, so batch tests drive begin without two mounts.
    static FAKE_MOUNTINFO: RefCell<Option<String>> = const { RefCell::new(None) };
    static FAIL_FILE: Cell<bool> = const { Cell::new(false) };
    static FAIL_DIR: Cell<bool> = const { Cell::new(false) };
    static FILE_FLUSHES: Cell<usize> = const { Cell::new(0) };
    static DIR_FLUSHES: Cell<usize> = const { Cell::new(0) };
    // The write leg answering EINVAL, so the copy falls back to the final fsync.
    static FAIL_RANGE_WRITE: Cell<bool> = const { Cell::new(false) };
    // The wait leg answering this errno, 0 for no failure.
    static FAIL_RANGE_WAIT: Cell<i32> = const { Cell::new(0) };
    // Completed slice waits, so a test pins reports against waits.
    static RANGE_WAITS: Cell<usize> = const { Cell::new(0) };
    // Range calls in order, so a test pins the pipelined sequence.
    static RANGE_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    // The syncfs leg answering this errno, 0 for no failure, so a batch confirm keeps every source.
    static FAIL_SYNCFS: Cell<i32> = const { Cell::new(0) };
    // The clone leg answering failure, so a batch confirm keeps every source after draining.
    static FAIL_CLONE: Cell<bool> = const { Cell::new(false) };
    // Completed syncfs calls, so a batch pins one per confirm.
    static SYNCFS_FLUSHES: Cell<usize> = const { Cell::new(0) };
    // Files handed to a close wave on the calling thread, so closes on background threads still count.
    static RELEASES: Cell<usize> = const { Cell::new(0) };
    // Release, join, syncfs, folder and removal steps in order, so a batch pins its sequence.
    static ORDER_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    // Close waves held open behind a gate until their join, so a test sees descriptors still open behind the next batch.
    static HOLD_WAVES: Cell<bool> = const { Cell::new(false) };
    // Descriptors still held or unjoined where an operation reports, None until read.
    static AT_REPORT: Cell<Option<usize>> = const { Cell::new(None) };
    // How many touch calls named a directory, flushed or not: the merge layer must add none of its own.
    static TOUCH_CALLS: Cell<usize> = const { Cell::new(0) };
}

// errno 22 means no range writeback, not a lost byte; errno 5 is a drive refusing bytes.
pub(crate) const EINVAL: i32 = 22;
#[cfg(test)]
pub(crate) const EIO: i32 = 5;

// Sample input: "fuse.rclone" trues, "fuse.sshfs" falses.
pub fn fstype_is_rclone(fstype: &str) -> bool {
    fstype.to_ascii_lowercase().contains("rclone")
}

// Sample input: "vfat" trues, "ext4" falses.
pub fn fat_name_is_durable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "vfat" || lower == "exfat" || lower == "ntfs" || lower == "ntfs3"
}

// Linux statfs magics for removable Windows filesystems; MSDOS and EXFAT match linux/magic.h.
const MSDOS_SUPER_MAGIC: i64 = 0x4D44;
const EXFAT_SUPER_MAGIC: i64 = 0x2011BAB0;
// NTFS legacy driver magic and the ntfs3 magic from fs/ntfs3/super.c (lowercase sftn).
const NTFS_SUPER_MAGIC: i64 = 0x5346544E;
const NTFS3_SUPER_MAGIC: i64 = 0x7366746e;

// Sample input: 0x4D44 trues, 0xEF53 falses.
pub fn fat_magic_is_durable(magic: i64) -> bool {
    magic == MSDOS_SUPER_MAGIC || magic == EXFAT_SUPER_MAGIC || magic == NTFS_SUPER_MAGIC || magic == NTFS3_SUPER_MAGIC
}

#[cfg(test)]
fn forced(dest: &Path) -> bool {
    FORCE.with(|v| v.borrow().iter().any(|p| dest == p || dest.starts_with(p)))
}

#[cfg(not(test))]
fn forced(_dest: &Path) -> bool {
    false
}

// A test counts every flush and can make one fail; a release build always answers "not failed".
#[cfg(test)]
fn seam_flush(dir: bool) -> bool {
    if dir {
        DIR_FLUSHES.with(|v| v.set(v.get() + 1));
        FAIL_DIR.with(|v| v.get())
    } else {
        FILE_FLUSHES.with(|v| v.set(v.get() + 1));
        FAIL_FILE.with(|v| v.get())
    }
}

#[cfg(not(test))]
fn seam_flush(_dir: bool) -> bool {
    false
}

// Rclone lands in its cache first, so an fsync here would force its background upload now.
pub fn dest_is_rclone(dest: &Path) -> bool {
    std::fs::read_to_string("/proc/self/mountinfo").is_ok_and(|body| {
        crate::backend::mountinfo::mount_entry_in(dest, &body).is_some_and(|e| fstype_is_rclone(&e.fstype))
    })
}

// True when the destination needs its bytes confirmed: usb, phone, network, a removable disk, or vfat/exfat/ntfs.
pub fn dest_is_durable(dest: &Path) -> bool {
    if forced(dest) {
        return true;
    }
    if !crate::backend::extclass::classify(dest).is_empty() {
        return true;
    }
    if let Ok(body) = std::fs::read_to_string("/proc/self/mountinfo") {
        if let Some(e) = crate::backend::mountinfo::mount_entry_in(dest, &body) {
            if entry_is_durable_under(Path::new("/sys"), Path::new("/dev"), &e) {
                return true;
            }
        }
    }
    if crate::backend::fsinfo::magic_of(dest).is_some_and(fat_magic_is_durable) {
        return true;
    }
    if crate::backend::fsinfo::read(dest).is_some_and(|i| fat_name_is_durable(&i.name)) {
        return true;
    }
    false
}

// A FAT name or a pull-out disk under these sysfs and dev roots; tests pass their own sample roots.
pub(crate) fn entry_is_durable_under(sys: &Path, dev: &Path, e: &crate::backend::mountinfo::MountEntry) -> bool {
    fat_name_is_durable(&e.fstype) || crate::backend::extclass::removable::removable_under(sys, dev, e)
}

// Sample input: "/dev/sda1" trues, "remote:" falses.
fn source_is_block(source: &str) -> bool {
    source.starts_with("/dev/")
}

// Sample input: ("/media/stick", "vfat /dev/sda1") trues, ("cifs //nas/media") falses.
pub(crate) fn batch_syncfs_for(dest: &Path, body: &str, durable: bool, rclone: bool) -> bool {
    if !durable || rclone {
        return false;
    }
    match crate::backend::mountinfo::mount_entry_in(dest, body) {
        Some(e) => source_is_block(&e.source) && fat_name_is_durable(&e.fstype),
        None => false,
    }
}

fn rclone_in(dest: &Path, body: &str) -> bool {
    crate::backend::mountinfo::mount_entry_in(dest, body).is_some_and(|e| fstype_is_rclone(&e.fstype))
}

// The done line's own words for a folder the drive would not confirm, printable as-is.
pub const DIR_UNCONFIRMED: &str = "copied, but the drive did not confirm the folder";

// 64 held files bound one unconfirmed batch, the same bound movebatch closes on.
pub(crate) const HELD_CAP: usize = 64;
// One confirm slice of held bytes closes a batch early, so files just under a slice never count far ahead of the drive.
pub(crate) const HELD_BYTES: u64 = crate::backend::copyfile::CONFIRM_BYTES;

// One batch being copied and confirmed plus the wave still closing behind it, the most descriptors a move holds open.
#[cfg(test)]
pub(crate) const MAX_OPEN_HELD: usize = 2 * HELD_CAP;

// The done line's own words for a copy onto rclone, printable as-is.
pub const RCLONE_NOTE: &str = "rclone uploads them in the background";

// The closes of one confirmed batch, running behind the next batch; joined before the next wave and before the result.
#[derive(Default)]
struct Wave {
    closers: Vec<std::thread::JoinHandle<std::io::Result<()>>>,
    // A file this wave closed inline answered its close with an error.
    failed: bool,
    // Test builds only: the gate holding this wave's closes open until its join opens it, so a test never hangs on it.
    #[cfg(test)]
    gate: Option<Arc<Gate>>,
}

#[cfg(test)]
struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
}

#[cfg(test)]
impl Gate {
    fn new() -> Gate {
        Gate { open: Mutex::new(false), opened: Condvar::new() }
    }

    fn wait(&self) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        while !*open {
            open = self.opened.wait(open).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn release(&self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.opened.notify_all();
    }
}

impl Wave {
    // One thread per file, so a 100 ms vfat close never waits on another; a spawn that fails leaves its file to a checked close inline.
    fn start(files: Vec<std::fs::File>) -> Wave {
        #[cfg(test)]
        let gate = HOLD_WAVES.with(|v| v.get()).then(|| Arc::new(Gate::new()));
        let checked = super::copyfile::landed::closer();
        let mut closers = Vec::with_capacity(files.len());
        let mut failed = false;
        for file in files {
            let slot = Arc::new(Mutex::new(Some(file)));
            let held = Arc::clone(&slot);
            let close_one = checked.clone();
            #[cfg(test)]
            let behind = gate.clone();
            let close = move || {
                #[cfg(test)]
                if let Some(gate) = &behind {
                    gate.wait();
                }
                held.lock().unwrap_or_else(|e| e.into_inner()).take().map_or(Ok(()), close_one)
            };
            match std::thread::Builder::new().spawn(close) {
                Ok(closer) => closers.push(closer),
                Err(_) => failed |= slot.lock().unwrap_or_else(|e| e.into_inner()).take().is_some_and(|file| checked(file).is_err()),
            }
        }
        Wave { closers, failed, #[cfg(test)] gate }
    }
}

// One operation's durability, created from its destination and carried down through Progress.
pub struct Durability {
    pub durable: bool,
    pub rclone: bool,
    pub file_failed: bool,
    pub batch_syncfs: bool,
    // True when the card's Rename answer covers this FAT destination, so a tree writes each UTF-8 name below the top as fatnames::sanitize gives it and any other name as it is.
    pub rename_fat: bool,
    // A failed settle stays failed, so later confirms keep every source.
    unsettled: bool,
    // A held file answered its close with an error, so the done line says the drive did not confirm a file.
    closes_failed: bool,
    held: Vec<std::fs::File>,
    held_bytes: u64,
    wave: Wave,
    touched: HashSet<PathBuf>,
    last: Option<PathBuf>,
}

impl Durability {
    // Classified once per operation, never per file; rclone stays non-durable to keep its upload in the background.
    pub fn begin(dest: &Path) -> Durability {
        #[cfg(test)]
        if let Some(body) = FAKE_MOUNTINFO.with(|v| v.borrow().clone()) {
            return Self::begin_from(dest, Some(&body));
        }
        let body = std::fs::read_to_string("/proc/self/mountinfo").ok();
        Self::begin_from(dest, body.as_deref())
    }

    // Sample input: Some("... vfat /dev/sda1 ...") batches a durable dest, None never does without a mount table.
    fn begin_from(dest: &Path, mountinfo: Option<&str>) -> Durability {
        let real = mountinfo.map_or_else(|| dest.to_path_buf(), |body| crate::backend::extclass::resolved_in(dest, body));
        let rclone = mountinfo.map_or_else(|| dest_is_rclone(dest), |body| rclone_in(dest, body) || rclone_in(&real, body));
        let durable = !rclone && dest_is_durable(dest);
        let batch_syncfs = match mountinfo {
            Some(body) => batch_syncfs_for(dest, body, durable, rclone) || batch_syncfs_for(&real, body, durable, rclone),
            None => false,
        };
        Durability { durable, rclone, file_failed: false, batch_syncfs, rename_fat: false, unsettled: false, closes_failed: false, held: Vec::new(),
            held_bytes: 0, wave: Wave::default(), touched: HashSet::new(), last: None }
    }

    // A landed file stays open until its batch confirms; 64 closes or one slice of still unconfirmed bytes land with one syncfs.
    pub fn hold(&mut self, file: std::fs::File, unconfirmed: u64) {
        self.held_bytes += unconfirmed;
        self.held.push(file);
        if self.held.len() >= HELD_CAP || self.held_bytes >= HELD_BYTES {
            // Never more than one batch ahead of the drive; a failed settle stays sticky inside settle_held.
            let _ = self.settle_held();
        }
    }

    // Confirm while every original is open, preserving the earliest file's pre-write error baseline.
    fn settle_held(&mut self) -> std::io::Result<()> {
        let had = !self.held.is_empty();
        if !had {
            if self.unsettled {
                return Err(std::io::Error::other("unconfirmed batch"));
            }
            return Ok(());
        }
        if self.unsettled {
            self.release_held();
            return Err(std::io::Error::other("unconfirmed batch"));
        }
        if !self.batch_syncfs {
            self.release_held();
            return Ok(());
        }
        // The clone shares the first file's baseline and drops before its original's final close.
        #[cfg(test)]
        if FAIL_CLONE.with(|v| v.get()) {
            self.release_held();
            self.unsettled = true;
            return Err(std::io::Error::other("unconfirmed batch"));
        }
        let clone = match self.held.first().and_then(|f| f.try_clone().ok()) {
            Some(c) => c,
            None => {
                self.release_held();
                self.unsettled = true;
                return Err(std::io::Error::other("unconfirmed batch"));
            }
        };
        #[cfg(test)]
        ORDER_LOG.with(|v| v.borrow_mut().push("clone".to_string()));
        let result = syncfs_fd(&clone);
        drop(clone);
        #[cfg(test)]
        ORDER_LOG.with(|v| v.borrow_mut().push("clone-drop".to_string()));
        self.release_held();
        // A failed confirm still hands every file to a wave and stays sticky for every later source.
        if result.is_err() {
            self.unsettled = true;
        }
        result
    }

    // The confirmed batch's closes run behind the next batch: the wave before it is joined first, so two batches of descriptors at most stay open.
    pub fn release_held(&mut self) {
        if self.held.is_empty() {
            return;
        }
        self.join_wave();
        let files: Vec<std::fs::File> = std::mem::take(&mut self.held);
        self.held_bytes = 0;
        #[cfg(test)]
        {
            RELEASES.with(|v| v.set(v.get() + files.len()));
            ORDER_LOG.with(|v| v.borrow_mut().extend(files.iter().map(|_| "release".to_string())));
        }
        self.wave = Wave::start(files);
    }

    // A close that failed, or a closer that panicked, fails every later confirm and the done line; its batch's syncfs already confirmed its bytes.
    fn join_wave(&mut self) {
        if std::mem::take(&mut self.wave.failed) {
            self.close_failed();
        }
        if self.wave.closers.is_empty() {
            return;
        }
        #[cfg(test)]
        {
            ORDER_LOG.with(|v| v.borrow_mut().push("join".to_string()));
            if let Some(gate) = self.wave.gate.take() {
                gate.release();
            }
        }
        let mut closed = true;
        for closer in self.wave.closers.drain(..) {
            closed &= matches!(closer.join(), Ok(Ok(())));
        }
        if !closed {
            self.close_failed();
        }
    }

    fn close_failed(&mut self) {
        self.unsettled = true;
        self.closes_failed = true;
    }

    // Everything this operation holds is closed and joined, so an eject right after is never refused.
    fn drain(&mut self) {
        self.release_held();
        self.join_wave();
    }

    // A test pins the held count without touching the descriptors.
    #[cfg(test)]
    pub(crate) fn held_len(&self) -> usize {
        self.held.len()
    }

    // One entry per directory however many files land in it, covering a tree that revisits a parent.
    pub fn touch(&mut self, dir: &Path) {
        #[cfg(test)]
        TOUCH_CALLS.with(|v| v.set(v.get() + 1));
        if !self.durable {
            return;
        }
        if self.last.as_deref() == Some(dir) {
            return;
        }
        self.last = Some(dir.to_path_buf());
        self.touched.insert(dir.to_path_buf());
    }

    pub fn note_file_failed(&mut self) {
        self.file_failed = true;
    }

    // A cancelled tree removed its own folders, so they leave the touched set with it.
    pub fn forget_tree(&mut self, root: &Path) {
        self.touched.retain(|p| p != root && !p.starts_with(root));
        if self.last.as_deref().is_some_and(|l| l == root || l.starts_with(root)) {
            self.last = None;
        }
    }

    // Deepest first, so a child's entry is confirmed before its parent's.
    fn ordered(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.touched.iter().cloned().collect();
        dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        dirs
    }

    // Only dst's touched folders; held files settle first so no source goes before its bytes.
    pub fn flush_dirs_for(&mut self, dst: &Path) -> std::io::Result<()> {
        self.settle_held()?;
        let mut first: Option<std::io::Error> = None;
        for dir in self.ordered() {
            if dir.starts_with(dst) || Some(dir.as_path()) == dst.parent() {
                if let Err(e) = fsync_dir(&dir) {
                    first.get_or_insert(e);
                }
            }
        }
        match first {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    // One confirm for a whole batch; held files settle first so no source goes before its bytes.
    pub fn flush_dirs_for_many(&mut self, dsts: &[PathBuf]) -> std::io::Result<()> {
        self.settle_held()?;
        let mut first: Option<std::io::Error> = None;
        for dir in self.ordered() {
            let wanted = dsts.iter().any(|dst| dir.starts_with(dst) || Some(dir.as_path()) == dst.parent());
            if wanted {
                if let Err(e) = fsync_dir(&dir) {
                    first.get_or_insert(e);
                }
            }
        }
        match first {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    // Every touched directory, best effort; held files settle first so no source goes early.
    pub fn flush_dirs(&mut self) -> std::io::Result<()> {
        self.settle_held()?;
        let mut first: Option<std::io::Error> = None;
        for dir in self.ordered() {
            if let Err(e) = fsync_dir(&dir) {
                if first.is_none() {
                    first = Some(e);
                }
            }
        }
        match first {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl Drop for Durability {
    // Whatever ends an operation, an early return or a panic included, leaves no descriptor open on the drive.
    fn drop(&mut self) {
        self.drain();
    }
}

// Counted through this seam, never timed: tests assert the counts.
pub fn fsync_file(f: &std::fs::File) -> std::io::Result<()> {
    #[cfg(test)]
    RANGE_LOG.with(|v| v.borrow_mut().push("fsync".to_string()));
    if seam_flush(false) {
        return Err(std::io::Error::other("simulated fsync failure"));
    }
    f.sync_all()
}

// sync_file_range(2) writes one slice back without the whole-file wait, so the card counts confirmed bytes mid-file.
const SYNC_FILE_RANGE_WAIT_BEFORE: u32 = 1;
const SYNC_FILE_RANGE_WRITE: u32 = 2;
const SYNC_FILE_RANGE_WAIT_AFTER: u32 = 4;

// std already links the system libc, so the one symbol is declared here rather than taking a crate.
extern "C" {
    fn sync_file_range(fd: i32, offset: i64, nbytes: i64, flags: u32) -> i32;
    fn syncfs(fd: i32) -> i32;
}

// One written slice starts its writeback without waiting, so the next slice writes while it flies.
pub fn sync_range_write(f: &std::fs::File, offset: u64, len: u64) -> std::io::Result<()> {
    #[cfg(test)]
    RANGE_LOG.with(|v| v.borrow_mut().push(format!("write {offset}")));
    #[cfg(test)]
    if FAIL_RANGE_WRITE.with(|v| v.get()) {
        return Err(std::io::Error::from_raw_os_error(EINVAL));
    }
    if seam_flush(false) {
        return Err(std::io::Error::other("simulated fsync failure"));
    }
    use std::os::unix::io::AsRawFd;
    let rc = unsafe { sync_file_range(f.as_raw_fd(), offset as i64, len as i64, SYNC_FILE_RANGE_WRITE) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

// One written slice is confirmed to the drive; a failure refuses the bytes the same way an fsync failure does.
pub fn sync_range(f: &std::fs::File, offset: u64, len: u64) -> std::io::Result<()> {
    #[cfg(test)]
    RANGE_LOG.with(|v| v.borrow_mut().push(format!("wait {offset}")));
    #[cfg(test)]
    {
        let errno = FAIL_RANGE_WAIT.with(|v| v.get());
        if errno != 0 {
            return Err(std::io::Error::from_raw_os_error(errno));
        }
    }
    if seam_flush(false) {
        return Err(std::io::Error::other("simulated fsync failure"));
    }
    use std::os::unix::io::AsRawFd;
    let flags = SYNC_FILE_RANGE_WAIT_BEFORE | SYNC_FILE_RANGE_WRITE | SYNC_FILE_RANGE_WAIT_AFTER;
    let rc = unsafe { sync_file_range(f.as_raw_fd(), offset as i64, len as i64, flags) };
    if rc == 0 {
        #[cfg(test)]
        RANGE_WAITS.with(|v| v.set(v.get() + 1));
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

// EINVAL, ESPIPE, ENOSYS and EOPNOTSUPP say the filesystem has no range writeback, not that a byte was lost.
pub fn range_unsupported(e: &std::io::Error) -> bool {
    const ESPIPE: i32 = 29;
    const ENOSYS: i32 = 38;
    const EOPNOTSUPP: i32 = 95;
    matches!(e.raw_os_error(), Some(EINVAL) | Some(ESPIPE) | Some(ENOSYS) | Some(EOPNOTSUPP))
}

pub fn fsync_dir(path: &Path) -> std::io::Result<()> {
    if seam_flush(true) {
        return Err(std::io::Error::other("simulated fsync failure"));
    }
    // Logged after the counting seam, so a failed flush never reads as confirmed.
    #[cfg(test)]
    ORDER_LOG.with(|v| v.borrow_mut().push("dir".to_string()));
    // A folder deep in a copied tree can have a path past PATH_MAX; it is opened a stretch at a time.
    crate::backend::copyfile::deep::open_long(path)?.sync_all()
}

// One filesystem-wide confirm on the pre-write fd, before vfat's final writable closes.
fn syncfs_fd(file: &std::fs::File) -> std::io::Result<()> {
    // Refused before counting, so a failed confirm never reads as confirmed.
    #[cfg(test)]
    {
        let errno = FAIL_SYNCFS.with(|v| v.get());
        if errno != 0 {
            return Err(std::io::Error::from_raw_os_error(errno));
        }
    }
    use std::os::unix::io::AsRawFd;
    let rc = unsafe { syncfs(file.as_raw_fd()) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // Counted and logged only after the seam and the syscall succeed, like fsync_dir.
    #[cfg(test)]
    {
        SYNCFS_FLUSHES.with(|v| v.set(v.get() + 1));
        ORDER_LOG.with(|v| v.borrow_mut().push("syncfs".to_string()));
    }
    Ok(())
}

// Test-only path probe; production confirms on the held-file clone.
#[cfg(test)]
pub fn syncfs_dir(path: &Path) -> std::io::Result<()> {
    let file = std::fs::File::open(path)?;
    syncfs_fd(&file)
}

// Sample input: "smb-share:server=nas,share=media" answers "media".
fn gvfs_drive_name(root: &Path) -> Option<String> {
    let name = root.file_name()?.to_str()?;
    for key in ["share=", "server=", "host="] {
        if let Some(at) = name.find(key) {
            let rest = &name[at + key.len()..];
            let end = rest.find(',').unwrap_or(rest.len());
            let value = rest[..end].trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

// Sample input: dest "/run/media/gm/128GB/photos" with mount "/run/media/gm/128GB" answers "128GB".
pub fn drive_name_in(dest: &Path, body: &str) -> String {
    if let Some(root) = crate::backend::extclass::gvfs_root(dest) {
        if let Some(name) = gvfs_drive_name(&root) {
            return name;
        }
    }
    if let Some(entry) = crate::backend::mountinfo::mount_entry_in(dest, body) {
        if let Some(name) = entry.mount.file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty()) {
            return name.to_string();
        }
    }
    dest.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| dest.to_string_lossy().into_owned())
}

// Sample input: /run/media/gm/128GB/photos answers "128GB".
pub fn drive_name(dest: &Path) -> String {
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    drive_name_in(dest, &body)
}

// Sample output: {"t":"transferprogress","id":1,"index":0,"name":"","bytes":0,"total":0,"scanned":0,"phase":"writing","drive":"128GB"}
pub fn writing_line(id: usize, drive: &str) -> String {
    format!(r#"{{"t":"transferprogress","id":{},"index":0,"name":"","bytes":0,"total":0,"scanned":0,"phase":"writing","drive":"{}"}}"#, id, crate::json::escape(drive))
}

pub struct Finish {
    pub ok: bool,
    pub note: String,
}

// After the last landed file: one writing line, then the settled batch syncfs, then every touched directory, then the last close wave joined.
pub fn finish(id: usize, tx: &std::sync::mpsc::Sender<crate::backend::opsreq::OpMsg>, durability: &mut Durability, dest: &Path, landed: usize) -> Finish {
    let finished = confirm_finish(id, tx, durability, dest, landed);
    // The done line goes out after this returns, so no descriptor may stay open on the drive past it.
    durability.drain();
    // A held file whose close failed after its batch confirmed leaves the whole result unconfirmed.
    if durability.closes_failed {
        return Finish { ok: false, note: super::copyfile::landed::FILE_UNCONFIRMED.to_string() };
    }
    finished
}

fn confirm_finish(id: usize, tx: &std::sync::mpsc::Sender<crate::backend::opsreq::OpMsg>, durability: &mut Durability, dest: &Path, landed: usize) -> Finish {
    // Nothing to confirm sends no syncfs; finish still drains the held descriptors, so an eject never waits on one.
    if landed == 0 {
        return Finish { ok: false, note: String::new() };
    }
    if durability.rclone {
        return Finish { ok: false, note: RCLONE_NOTE.to_string() };
    }
    if !durability.durable {
        return Finish { ok: false, note: String::new() };
    }
    let _ = tx.send(crate::backend::opsreq::OpMsg::Meta { line: writing_line(id, &drive_name(dest)) });
    if durability.file_failed {
        return Finish { ok: false, note: String::new() };
    }
    // flush_dirs settles held files first, so every file is handed to a wave before the folders confirm.
    match durability.flush_dirs() {
        Ok(()) => Finish { ok: true, note: String::new() },
        Err(_) => Finish { ok: false, note: DIR_UNCONFIRMED.to_string() },
    }
}

#[cfg(test)]
pub fn test_reset() {
    FORCE.with(|v| v.borrow_mut().clear());
    FAKE_MOUNTINFO.with(|v| *v.borrow_mut() = None);
    crate::backend::extclass::test_reset_link_reads();
    FAIL_FILE.with(|v| v.set(false));
    FAIL_DIR.with(|v| v.set(false));
    FILE_FLUSHES.with(|v| v.set(0));
    DIR_FLUSHES.with(|v| v.set(0));
    FAIL_RANGE_WRITE.with(|v| v.set(false));
    FAIL_RANGE_WAIT.with(|v| v.set(0));
    RANGE_WAITS.with(|v| v.set(0));
    RANGE_LOG.with(|v| v.borrow_mut().clear());
    FAIL_SYNCFS.with(|v| v.set(0));
    FAIL_CLONE.with(|v| v.set(false));
    SYNCFS_FLUSHES.with(|v| v.set(0));
    RELEASES.with(|v| v.set(0));
    ORDER_LOG.with(|v| v.borrow_mut().clear());
    HOLD_WAVES.with(|v| v.set(false));
    AT_REPORT.with(|v| v.set(None));
    TOUCH_CALLS.with(|v| v.set(0));
}

#[cfg(test)]
pub fn test_reset_counts() {
    FAIL_FILE.with(|v| v.set(false));
    FAIL_DIR.with(|v| v.set(false));
    FILE_FLUSHES.with(|v| v.set(0));
    DIR_FLUSHES.with(|v| v.set(0));
    FAIL_RANGE_WRITE.with(|v| v.set(false));
    FAIL_RANGE_WAIT.with(|v| v.set(0));
    RANGE_WAITS.with(|v| v.set(0));
    RANGE_LOG.with(|v| v.borrow_mut().clear());
    FAIL_SYNCFS.with(|v| v.set(0));
    FAIL_CLONE.with(|v| v.set(false));
    SYNCFS_FLUSHES.with(|v| v.set(0));
    RELEASES.with(|v| v.set(0));
    ORDER_LOG.with(|v| v.borrow_mut().clear());
}

#[cfg(test)]
pub fn test_counts() -> (usize, usize) {
    (FILE_FLUSHES.with(|v| v.get()), DIR_FLUSHES.with(|v| v.get()))
}

// A batch_syncfs confirm counts one syncfs beside its file and folder counts.
#[cfg(test)]
pub fn test_syncfs_count() -> usize {
    SYNCFS_FLUSHES.with(|v| v.get())
}

// Closes counted on the calling thread, so scoped threads never hide one.
#[cfg(test)]
pub fn test_releases() -> usize {
    RELEASES.with(|v| v.get())
}

// Waves started from here on stay open until their join, so a test sees descriptors still open behind the next batch.
#[cfg(test)]
pub(crate) fn test_hold_waves(hold: bool) {
    HOLD_WAVES.with(|v| v.set(hold));
}

// Called where an operation reports its result: descriptors still held or whose closers are not yet joined.
#[cfg(test)]
pub(crate) fn test_note_report(durability: &Durability) {
    AT_REPORT.with(|v| v.set(Some(durability.held.len() + durability.wave.closers.len())));
}

#[cfg(test)]
pub(crate) fn test_open_at_report() -> Option<usize> {
    AT_REPORT.with(|v| v.get())
}

// This process's open descriptors naming a file under dir, so a test counts what an eject would be refused for.
#[cfg(test)]
pub(crate) fn test_open_under(dir: &Path) -> usize {
    let fds = std::fs::read_dir("/proc/self/fd").map(|d| d.flatten().collect::<Vec<_>>()).unwrap_or_default();
    fds.iter().filter_map(|fd| std::fs::read_link(fd.path()).ok()).filter(|target| target.starts_with(dir)).count()
}

// Release, join, syncfs, folder and removal steps in the order they ran.
#[cfg(test)]
pub fn test_order() -> Vec<String> {
    ORDER_LOG.with(|v| v.borrow().clone())
}

// One ordered step from outside this module, so a batch pins a removal after its folder.
#[cfg(test)]
pub(crate) fn test_log(step: &str) {
    ORDER_LOG.with(|v| v.borrow_mut().push(step.to_string()));
}

#[cfg(test)]
pub fn test_range_waits() -> usize {
    RANGE_WAITS.with(|v| v.get())
}

#[cfg(test)]
pub fn test_range_log() -> Vec<String> {
    RANGE_LOG.with(|v| v.borrow().clone())
}

// The write leg answers EINVAL, so the copy falls back to the final fsync.
#[cfg(test)]
pub fn test_set_fail_range_write(fail: bool) {
    FAIL_RANGE_WRITE.with(|v| v.set(fail));
}

// The wait leg answers this errno, 0 clears it.
#[cfg(test)]
pub fn test_set_fail_range_wait(errno: i32) {
    FAIL_RANGE_WAIT.with(|v| v.set(errno));
}

// Test builds only: how many touch calls named a directory, flushed or not.
#[cfg(test)]
pub fn test_touch_calls() -> usize {
    TOUCH_CALLS.with(|v| v.get())
}

#[cfg(test)]
pub fn test_set_fail(fail: bool) {
    FAIL_FILE.with(|v| v.set(fail));
    FAIL_DIR.with(|v| v.set(fail));
}

#[cfg(test)]
pub fn test_set_fail_dirs(fail: bool) {
    FAIL_DIR.with(|v| v.set(fail));
}

#[cfg(test)]
pub fn test_set_fail_files(fail: bool) {
    FAIL_FILE.with(|v| v.set(fail));
}

// The syncfs leg answers this errno, an unplug or a read-only remount included, so a batch keeps every source; 0 clears it.
#[cfg(test)]
pub(crate) fn test_set_syncfs_errno(errno: i32) {
    FAIL_SYNCFS.with(|v| v.set(errno));
}

// The clone leg answers failure, so a batch keeps every source after draining.
#[cfg(test)]
pub fn test_set_fail_clone(fail: bool) {
    FAIL_CLONE.with(|v| v.set(fail));
}

// Injected mountinfo text answers the next begin, None takes it away again.
#[cfg(test)]
pub fn test_set_fake_mountinfo(body: Option<&str>) {
    FAKE_MOUNTINFO.with(|v| *v.borrow_mut() = body.map(|s| s.to_string()));
}

#[cfg(test)]
pub fn test_mark_durable(path: &Path) {
    FORCE.with(|v| v.borrow_mut().push(path.to_path_buf()));
}

#[cfg(test)]
#[path = "durable_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "durable_bytes_tests.rs"]
mod bytes_tests;
