use std::os::unix::net::UnixDatagram;

pub const UNAVAILABLE: i32 = 78;
pub const MESSAGE: &str = "flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui";

pub fn unavailable() {
    eprintln!("{MESSAGE}");
    let Ok(socket) = UnixDatagram::unbound() else {
        return;
    };
    let _ = socket.set_nonblocking(true);
    let syslog = format!("<11>flea[{}]: {MESSAGE}", std::process::id());
    if socket.send_to(syslog.as_bytes(), "/dev/log").is_err() {
        let native = format!("PRIORITY=3\nSYSLOG_IDENTIFIER=flea\nMESSAGE={MESSAGE}\n");
        let _ = socket.send_to(native.as_bytes(), "/run/systemd/journal/socket");
    }
}
