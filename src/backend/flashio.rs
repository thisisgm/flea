// The flasher's raw I/O: aligned buffers, positioned reads and writes, the O_DIRECT flag and the final flush.
use crate::backend::flash::DIRECT_ALIGN;
pub(crate) use crate::oflags::O_DIRECT;
use std::os::fd::AsRawFd;
use std::os::raw::{c_int, c_void};

extern "C" {
    fn pread(fd: c_int, buf: *mut c_void, count: usize, offset: i64) -> isize;
    fn pwrite(fd: c_int, buf: *const c_void, count: usize, offset: i64) -> isize;
    fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    fn fsync(fd: c_int) -> c_int;
}

// fcntl commands and the status flag, declared here rather than taking a crate.
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
// The errno a write answers when the drive is full.
const ENOSPC: i32 = 28;

// One window aligned to DIRECT_ALIGN inside an ordinary allocation, so O_DIRECT accepts it and the allocator frees it.
pub(crate) struct Aligned {
    backing: Vec<u8>,
    start: usize,
    len: usize,
}

impl Aligned {
    pub(crate) fn new(len: usize) -> Aligned {
        let backing = vec![0u8; len + DIRECT_ALIGN];
        let start = (DIRECT_ALIGN - backing.as_ptr() as usize % DIRECT_ALIGN) % DIRECT_ALIGN;
        Aligned { backing, start, len }
    }

    pub(crate) fn bytes(&mut self) -> &mut [u8] {
        &mut self.backing[self.start..self.start + self.len]
    }

    pub(crate) fn view(&self) -> &[u8] {
        &self.backing[self.start..self.start + self.len]
    }
}

// fcntl's F_GETFL answers -1 when it fails, which reads as no O_DIRECT.
pub(crate) fn direct_flag(flags: c_int) -> bool {
    flags >= 0 && flags & O_DIRECT != 0
}

pub(crate) fn direct_on(file: &std::fs::File) -> bool {
    direct_flag(unsafe { fcntl(file.as_raw_fd(), F_GETFL, 0) })
}

// The O_DIRECT flag without touching any other status flag; a failed F_GETFL changes nothing.
pub(crate) fn set_direct(file: &std::fs::File, on: bool) {
    let flags = unsafe { fcntl(file.as_raw_fd(), F_GETFL, 0) };
    if flags < 0 {
        return;
    }
    let next = if on { flags | O_DIRECT } else { flags & !O_DIRECT };
    if next != flags {
        unsafe { fcntl(file.as_raw_fd(), F_SETFL, next) };
    }
}

pub(crate) fn no_space(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(ENOSPC)
}

pub(crate) fn full_pwrite(fd: c_int, buf: &[u8], offset: u64) -> std::io::Result<()> {
    let mut at = 0;
    while at < buf.len() {
        let n = unsafe { pwrite(fd, buf[at..].as_ptr() as *const c_void, buf.len() - at, (offset + at as u64) as i64) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "no progress writing the drive"));
        }
        at += n as usize;
    }
    Ok(())
}

pub(crate) fn full_pread(fd: c_int, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    let mut at = 0;
    while at < buf.len() {
        let n = unsafe { pread(fd, buf[at..].as_mut_ptr() as *mut c_void, buf.len() - at, (offset + at as u64) as i64) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            break;
        }
        at += n as usize;
    }
    Ok(at)
}

// One write at an offset: whole blocks go down as they are, a ragged tail goes through the buffered flag.
pub(crate) fn write_at(dest: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<()> {
    let fd = dest.as_raw_fd();
    if buf.len() % DIRECT_ALIGN == 0 || !direct_on(dest) {
        return full_pwrite(fd, buf, offset);
    }
    set_direct(dest, false);
    let out = full_pwrite(fd, buf, offset);
    set_direct(dest, true);
    out
}

// The final flush: fsync on the block device sends the drive its cache flush.
pub(crate) fn flush(dest: &std::fs::File) -> std::io::Result<()> {
    if unsafe { fsync(dest.as_raw_fd()) } == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}
