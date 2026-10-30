// The clipboard owner: a source offering every type and holding the selection until cancelled or lost.
use super::control::{self, next_event, ROUNDTRIP_MS};
use super::protocol::*;
use super::format;
use super::wire::{self, Conn};
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::os::raw::c_int;
use std::time::{Duration, Instant};

// Block until the event or the descriptor arrives; the owner serves until it is replaced.
pub const WAIT_FOREVER: u32 = u32::MAX;
// One send never holds the serve loop past this, so a reader that stopped cannot wedge the owner.
const SEND_TIMEOUT_MS: u32 = 2000;

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

const POLLOUT: i16 = 4;

extern "C" {
    fn close(fd: c_int) -> c_int;
    // libc declares open variadic (the mode follows O_CREAT), and a newer rustc denies any other shape.
    fn open(path: *const std::os::raw::c_char, flags: c_int, ...) -> c_int;
    fn dup2(old: c_int, new: c_int) -> c_int;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: c_int) -> c_int;
    fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
}

const O_RDWR: c_int = 2;
const F_SETFL: c_int = 4;
const O_NONBLOCK: c_int = 0o4000;

// After ready the owner holds no pipe of its parent open; stdio is /dev/null from here.
pub fn detach_stdio() {
    let null = unsafe { open(c"/dev/null".as_ptr(), O_RDWR) };
    if null < 0 {
        return;
    }
    unsafe {
        dup2(null, 0);
        dup2(null, 1);
        dup2(null, 2);
        if null > 2 {
            close(null);
        }
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;

pub struct Owner {
    conn: Conn,
    bytes: HashMap<String, Vec<u8>>,
}

// A source offering every type and holding the selection; serve_owner answers its sends.
// With content, one file's own type and bytes are offered first, ahead of the path list.
pub fn own_with(mut conn: Conn, op: &str, paths: &[String], token: &str, content: Option<(String, Vec<u8>)>) -> Result<Owner, String> {
    let mut bytes = HashMap::new();
    let mut offered = Vec::new();
    if let Some((mime, body)) = content {
        offered.push(mime.clone());
        bytes.insert(mime, body);
    }
    for (mime, body) in format::payloads(op, paths, token) {
        offered.push(mime.to_string());
        bytes.insert(mime.to_string(), body);
    }
    let mut payload = Vec::new();
    wire::put_u32(&mut payload, OWNER_SOURCE);
    conn.send(MANAGER, MANAGER_CREATE_SOURCE, &payload, &[])?;
    for mime in &offered {
        payload.clear();
        wire::put_string(&mut payload, mime);
        conn.send(OWNER_SOURCE, SOURCE_OFFER, &payload, &[])?;
    }
    payload.clear();
    wire::put_u32(&mut payload, OWNER_DEVICE);
    wire::put_u32(&mut payload, SEAT);
    conn.send(MANAGER, MANAGER_GET_DEVICE, &payload, &[])?;
    payload.clear();
    wire::put_u32(&mut payload, OWNER_SOURCE);
    conn.send(OWNER_DEVICE, DEVICE_SET_SELECTION, &payload, &[])?;
    control::sync(&mut conn, OWNER_CALLBACK)?;
    loop {
        let event = next_event(&mut conn, ROUNDTRIP_MS)?;
        if event.sender == OWNER_CALLBACK && event.opcode == CALLBACK_DONE {
            return Ok(Owner { conn, bytes });
        }
    }
}

// Every send writes that type's bytes and closes; cancelled (or a lost server) ends the owner.
pub fn serve_owner(owner: &mut Owner) -> Result<(), String> {
    let mut offers: HashMap<u32, ()> = HashMap::new();
    loop {
        let event = next_event(&mut owner.conn, WAIT_FOREVER)?;
        // Superseded offers are destroyed on every selection, primary ones included.
        if event.sender == OWNER_DEVICE {
            match event.opcode {
                DEVICE_DATA_OFFER => {
                    let mut at = 0;
                    if let Some(id) = wire::get_u32(&event.body, &mut at) {
                        offers.insert(id, ());
                    }
                }
                DEVICE_SELECTION | DEVICE_PRIMARY_SELECTION => {
                    super::watch::retire_offers(&mut owner.conn, &mut offers, super::watch::selected_id(&event.body));
                }
                _ => {}
            }
            continue;
        }
        if event.sender != OWNER_SOURCE {
            continue;
        }
        if event.opcode == SOURCE_CANCELLED {
            return Ok(());
        }
        if event.opcode != SOURCE_SEND {
            continue;
        }
        let mut at = 0;
        let Some(mime) = wire::get_string(&event.body, &mut at) else {
            continue;
        };
        let Some(fd) = owner.conn.take_fd(ROUNDTRIP_MS)? else {
            continue;
        };
        if let Some(body) = owner.bytes.get(&mime) {
            // The close is the reader's EOF either way: an abandoned send still ends its wait.
            write_deadline(fd, body, SEND_TIMEOUT_MS);
        }
    }
}

// One send that never blocks past the deadline; the fd closes on every path so a stalled reader sees EOF.
fn write_deadline(fd: std::os::fd::OwnedFd, bytes: &[u8], timeout_ms: u32) {
    if unsafe { fcntl(fd.as_raw_fd(), F_SETFL, O_NONBLOCK) } != 0 {
        return;
    }
    let mut file = std::fs::File::from(fd);
    use std::io::Write;
    let end = Instant::now() + Duration::from_millis(timeout_ms as u64);
    let mut rest = bytes;
    while !rest.is_empty() {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        let mut p = PollFd { fd: file.as_raw_fd(), events: POLLOUT, revents: 0 };
        if unsafe { poll(&mut p, 1, left.as_millis().min(u32::MAX as u128) as c_int) } <= 0 {
            return;
        }
        match file.write(rest) {
            Ok(0) => return,
            Ok(n) => rest = &rest[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
    }
}
