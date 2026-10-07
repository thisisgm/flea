// X11 file clipboard adapter. xclip owns the GNOME file selection after Flea exits;
// a small Xlib watcher notices selection-owner changes for other Flea windows.
use super::{format, reply};
use crate::backend::opsreq::OpMsg;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::raw::{c_char, c_int, c_long, c_ulong};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Once};
use std::time::Duration;

#[link(name = "X11")]
extern "C" {
    fn XInitThreads() -> c_int;
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XInternAtom(display: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XGetSelectionOwner(display: *mut c_void, selection: c_ulong) -> c_ulong;
    fn XSetSelectionOwner(display: *mut c_void, selection: c_ulong, owner: c_ulong, time: c_ulong);
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XCreateSimpleWindow(display: *mut c_void, parent: c_ulong, x: c_int, y: c_int,
        width: u32, height: u32, border: u32, border_color: c_ulong, background: c_ulong) -> c_ulong;
    fn XDestroyWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XPending(display: *mut c_void) -> c_int;
    fn XNextEvent(display: *mut c_void, event: *mut c_void) -> c_int;
    fn XChangeProperty(display: *mut c_void, window: c_ulong, property: c_ulong,
        kind: c_ulong, format: c_int, mode: c_int, data: *const u8, count: c_int);
    fn XGetWindowProperty(display: *mut c_void, window: c_ulong, property: c_ulong,
        offset: c_long, length: c_long, delete: c_int, kind: c_ulong,
        actual_kind: *mut c_ulong, actual_format: *mut c_int,
        count: *mut c_ulong, remaining: *mut c_ulong, data: *mut *mut u8) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XSetErrorHandler(handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int>)
        -> Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int>;
    fn XSelectInput(display: *mut c_void, window: c_ulong, event_mask: c_long) -> c_int;
    fn setsid() -> c_int;
}

#[link(name = "Xfixes")]
extern "C" {
    fn XFixesQueryExtension(display: *mut c_void, event_base: *mut c_int, error_base: *mut c_int) -> c_int;
    fn XFixesSelectSelectionInput(display: *mut c_void, window: c_ulong, selection: c_ulong, mask: c_ulong);
}

static X11_INIT: Once = Once::new();
const MAX_PAYLOAD: u64 = 64 * 1024 * 1024;
const READ_WAIT: Duration = Duration::from_secs(3);
const WATCH_EVERY: Duration = Duration::from_millis(150);
const PROPERTY_CHANGE_MASK: c_long = 1 << 22;
const PROPERTY_NOTIFY: c_int = 28;

// A clipboard owner may disappear between querying its XID and reading its property.
// Xlib's default error handler exits the whole backend for that ordinary race.
unsafe extern "C" fn ignore_x_error(_display: *mut c_void, _event: *mut c_void) -> c_int { 0 }

struct Display {
    raw: *mut c_void,
    clipboard: c_ulong,
    token_atom: c_ulong,
    utf8_atom: c_ulong,
    watcher_window: c_ulong,
}

impl Display {
    fn open() -> Result<Self, String> {
        X11_INIT.call_once(|| unsafe {
            XInitThreads();
            XSetErrorHandler(Some(ignore_x_error));
        });
        let raw = unsafe { XOpenDisplay(std::ptr::null()) };
        if raw.is_null() {
            return Err("the X11 display could not be opened".to_string());
        }
        let clipboard = unsafe { XInternAtom(raw, c"CLIPBOARD".as_ptr(), 0) };
        let token_atom = unsafe { XInternAtom(raw, c"XFLEA_CLIP_TOKEN".as_ptr(), 0) };
        let utf8_atom = unsafe { XInternAtom(raw, c"UTF8_STRING".as_ptr(), 0) };
        Ok(Self { raw, clipboard, token_atom, utf8_atom, watcher_window: 0 })
    }

    fn owner(&self) -> c_ulong {
        unsafe { XGetSelectionOwner(self.raw, self.clipboard) }
    }

    fn clear(&self) -> bool {
        unsafe {
            XSetSelectionOwner(self.raw, self.clipboard, 0, 0);
            XSync(self.raw, 0);
        }
        self.owner() == 0
    }

    fn put_token(&self, owner: c_ulong, token: &str) -> bool {
        unsafe {
            XChangeProperty(self.raw, owner, self.token_atom, self.utf8_atom, 8, 0,
                token.as_ptr(), token.len() as c_int);
            XSync(self.raw, 0);
        }
        self.owner() == owner && self.get_token(owner) == Some(format!("x11:{token}"))
    }

    fn get_token(&self, owner: c_ulong) -> Option<String> {
        if owner == 0 { return None; }
        let (mut kind, mut format, mut count, mut remaining) = (0, 0, 0, 0);
        let mut data = std::ptr::null_mut();
        let status = unsafe { XGetWindowProperty(self.raw, owner, self.token_atom, 0, 16, 0,
            self.utf8_atom, &mut kind, &mut format, &mut count, &mut remaining, &mut data) };
        let value = if status == 0 && kind == self.utf8_atom && format == 8 && count == 32 && remaining == 0 && !data.is_null() {
            let bytes = unsafe { std::slice::from_raw_parts(data, count as usize) };
            std::str::from_utf8(bytes).ok().filter(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(|s| format!("x11:{s}"))
        } else { None };
        if !data.is_null() { unsafe { XFree(data.cast()); } }
        value
    }

    fn watch_selection(&mut self) -> Option<c_int> {
        let mut event_base = 0;
        let mut error_base = 0;
        if unsafe { XFixesQueryExtension(self.raw, &mut event_base, &mut error_base) } == 0 {
            return None;
        }
        let root = unsafe { XDefaultRootWindow(self.raw) };
        self.watcher_window = unsafe { XCreateSimpleWindow(self.raw, root, 0, 0, 1, 1, 0, 0, 0) };
        if self.watcher_window == 0 { return None; }
        // Owner set, owner-window destroy and client close all announce a new selection state.
        unsafe {
            XFixesSelectSelectionInput(self.raw, self.watcher_window, self.clipboard, 0b111);
            XSync(self.raw, 0);
        }
        Some(event_base)
    }

    fn watch_owner_token(&self, owner: c_ulong) {
        if owner != 0 {
            unsafe {
                XSelectInput(self.raw, owner, PROPERTY_CHANGE_MASK);
                XSync(self.raw, 0);
            }
        }
    }

    fn selection_event_pending(&self, event_base: c_int) -> bool {
        let mut changed = false;
        while unsafe { XPending(self.raw) } > 0 {
            // XEvent is a union of 24 longs; only its first int (type) is needed.
            let mut event = [0 as c_ulong; 24];
            unsafe { XNextEvent(self.raw, event.as_mut_ptr().cast()); }
            let kind = unsafe { *(event.as_ptr().cast::<c_int>()) };
            if kind == event_base || kind == PROPERTY_NOTIFY { changed = true; }
        }
        changed
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        unsafe {
            if self.watcher_window != 0 { XDestroyWindow(self.raw, self.watcher_window); }
            XCloseDisplay(self.raw);
        }
    }
}

fn valid_token(token: &str) -> bool {
    let Some(hex) = token.strip_prefix("x11:") else { return false; };
    hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn xclip_error(error: std::io::Error) -> String {
    format!("xclip could not run ({error})")
}

fn read_target(mime: &str) -> Result<Option<Vec<u8>>, String> {
    let mut child = Command::new("xclip")
        .args(["-selection", "clipboard", "-target", mime, "-out"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(xclip_error)?;
    let stdout = child.stdout.take().ok_or("xclip has no output")?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(MAX_PAYLOAD + 1).read_to_end(&mut bytes);
        let _ = tx.send((result, bytes));
    });
    let received = rx.recv_timeout(READ_WAIT);
    if received.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(xclip_error)?;
    let _ = reader.join();
    let (read, bytes) = received.map_err(|_| "the X11 clipboard read timed out".to_string())?;
    read.map_err(|e| format!("the X11 clipboard could not be read ({e})"))?;
    if bytes.len() as u64 > MAX_PAYLOAD {
        return Err("the X11 clipboard payload passed its 64 MiB cap".to_string());
    }
    Ok(status.success().then_some(bytes))
}

fn read_files(display: &Display, owner: c_ulong) -> Result<reply::Got, String> {
    let none = || reply::Got { clip: "none".into(), paths: Vec::new(), token: String::new(), skipped: 0 };
    if owner == 0 { return Ok(none()); }
    if let Some(bytes) = read_target(format::GNOME)? {
        if let Some((op, paths, skipped)) = format::parse_gnome(&bytes) {
            if !paths.is_empty() {
                format::check_path_cap(&paths)?;
                return Ok(reply::Got { clip: op, paths, token: display.get_token(owner).unwrap_or_default(), skipped });
            }
        }
    }
    if let Some(bytes) = read_target(format::URILIST)? {
        let (paths, skipped) = format::parse_urilist(&bytes);
        if !paths.is_empty() {
            format::check_path_cap(&paths)?;
            let cut = read_target(format::KDE_CUT)?.is_some_and(|bytes| format::parse_kde_cut(&bytes));
            return Ok(reply::Got { clip: if cut { "cut" } else { "copy" }.into(),
                paths, token: display.get_token(owner).unwrap_or_default(), skipped });
        }
    }
    Ok(none())
}

pub fn get() -> Result<reply::Got, String> {
    let display = Display::open()?;
    for _ in 0..2 {
        let owner = display.owner();
        let got = read_files(&display, owner)?;
        if display.owner() == owner { return Ok(got); }
    }
    Err("the X11 clipboard changed while it was being read".to_string())
}

pub fn set(op: &str, paths: &[String]) -> Result<String, String> {
    if !format::is_op(op) { return Err("the clipboard operation is copy or cut".to_string()); }
    format::validate_clip_paths(paths)?;
    if paths.is_empty() { return Err("the clipboard names no path".to_string()); }
    let display = Display::open()?;
    let before = display.owner();
    let mut command = Command::new("xclip");
    command.args(["-selection", "clipboard", "-target", format::GNOME, "-in", "-quiet"])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    // -quiet keeps xclip serving requests; detaching lets that selection outlive Flea.
    unsafe {
        command.pre_exec(|| if setsid() < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) });
    }
    let mut child = command.spawn().map_err(xclip_error)?;
    let payload = format::build_gnome(op, paths);
    let write = child.stdin.take().ok_or("xclip has no input")?.write_all(&payload);
    if let Err(error) = write {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("xclip could not accept the file selection ({error})"));
    }
    for _ in 0..30 {
        if let Some(status) = child.try_wait().map_err(xclip_error)? {
            return Err(format!("xclip stopped before publishing the file selection ({status})"));
        }
        let owner = display.owner();
        if owner != 0 && owner != before
            && read_files(&display, owner).is_ok_and(|got| got.clip == op && got.paths == paths)
            && display.owner() == owner {
            let token = format::make_token()?;
            if !display.put_token(owner, &token) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the X11 clipboard token could not be attached".to_string());
            }
            std::thread::spawn(move || { let _ = child.wait(); });
            return Ok(format!("x11:{token}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("xclip did not publish the file selection".to_string())
}

pub fn clear(token: &str) -> Result<bool, String> {
    if !valid_token(token) { return Ok(false); }
    let display = Display::open()?;
    let owner = display.owner();
    if display.get_token(owner).as_deref() != Some(token) || display.owner() != owner { return Ok(false); }
    Ok(display.clear())
}

pub fn clear_cut(paths: &[String]) -> Result<bool, String> {
    if paths.is_empty() { return Ok(false); }
    let display = Display::open()?;
    let owner = display.owner();
    let got = read_files(&display, owner)?;
    if got.clip != "cut" || got.paths != paths || display.owner() != owner { return Ok(false); }
    Ok(display.clear())
}

fn changed(got: &reply::Got) -> String {
    reply::reply_get(true, Some(got), "").replacen("\"op\":\"get\"", "\"op\":\"changed\"", 1)
        .replacen("\"ok\":true,", "", 1)
}

fn changed_error(error: &str) -> String {
    format!(r#"{{"t":"clip","op":"changed","clip":"none","paths":[],"token":"","skipped":0,"error":"{}"}}"#,
        crate::json::escape(error))
}

pub fn watch(replies: mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let mut display = match Display::open() {
            Ok(display) => display,
            Err(error) => {
                let line = changed_error(&error);
                let _ = replies.send(OpMsg::Meta { line });
                return;
            }
        };
        let event_base = display.watch_selection();
        let mut last_owner = c_ulong::MAX;
        let mut last_line = String::new();
        let mut ticks = 0;
        loop {
            let owner = display.owner();
            // Subscribe before reading: a token added after the read then triggers
            // PropertyNotify. If it was added during subscription, the read sees it.
            if owner != last_owner && event_base.is_some() { display.watch_owner_token(owner); }
            // On servers without XFixes, re-read each second: XIDs can be reused.
            let notified = event_base.is_some_and(|base| display.selection_event_pending(base));
            ticks += 1;
            if owner != last_owner || notified || (event_base.is_none() && ticks >= 7) {
                ticks = 0;
                last_owner = owner;
                let line = match read_files(&display, owner) {
                    Ok(got) if display.owner() == owner => changed(&got),
                    Ok(_) => { last_owner = c_ulong::MAX; std::thread::sleep(WATCH_EVERY); continue; }
                    Err(error) => changed_error(&error),
                };
                if line != last_line {
                    last_line = line.clone();
                    if replies.send(OpMsg::Meta { line }).is_err() { return; }
                }
            }
            std::thread::sleep(WATCH_EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_refuse_other_shapes() {
        assert!(valid_token("x11:ab12cd34ab12cd34ab12cd34ab12cd34"));
        for token in ["", "x11:", "x11:0", "x11:-1", "x11:gg", "deadbeef"] {
            assert!(!valid_token(token));
        }
    }

    #[test]
    fn changed_reply_keeps_the_clipboard_shape() {
        let got = reply::Got { clip: "cut".into(), paths: vec!["/tmp/a".into()], token: "x11:1a".into(), skipped: 0 };
        let line = changed(&got);
        assert!(line.contains("\"op\":\"changed\""));
        assert!(line.contains("\"clip\":\"cut\""));
        assert!(!line.contains("\"ok\""));
    }
}
