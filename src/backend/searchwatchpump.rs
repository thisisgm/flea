// The recursive watch wakes immediately and its eventfd interrupts an idle wait on close.
use super::searchwatch::Core;
use std::ffi::c_int;
use std::sync::Arc;

#[repr(C)]
struct PollFd { fd: c_int, events: i16, revents: i16 }
extern "C" {
    fn poll(fds: *mut PollFd, count: usize, timeout: c_int) -> c_int;
}

pub(crate) fn pump(core: Arc<Core>, fd: c_int, stop: c_int) {
    loop {
        let mut fds = [PollFd { fd, events: 1, revents: 0 }, PollFd { fd: stop, events: 1, revents: 0 }];
        let result = unsafe { poll(fds.as_mut_ptr(), fds.len(), -1) };
        if result < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted { continue; }
        if fds[1].revents != 0 { return; }
        if result < 0 || fds[0].revents & !1 != 0 { core.failed(); return; }
        core.flush();
    }
}
