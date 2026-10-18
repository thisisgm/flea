use super::capabilities::Color;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transport {
    #[default]
    Direct,
    Tmux { passthrough: bool, rgb: bool },
}
impl Transport {
    pub fn discover() -> Self {
        if std::env::var_os("TMUX").is_none_or(|v| v.is_empty()) { return Self::Direct; }
        let mut command = Command::new("tmux");
        command.args(["display-message", "-p"]);
        if let Some(pane) = std::env::var_os("TMUX_PANE").filter(|v| !v.is_empty()) { command.arg("-t").arg(pane); }
        command.arg("#{allow-passthrough}\n#{client_termfeatures}");
        discover_with(&mut command, Duration::from_millis(150))
    }
    pub fn publish(self) {
        OUTPUT_TRANSPORT.store(match self { Self::Direct => 0, Self::Tmux { passthrough: false, .. } => 1, Self::Tmux { passthrough: true, .. } => 2 }, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn graphics(self) -> bool {
        !matches!(self, Self::Tmux { passthrough: false, .. })
    }
    pub fn color(self) -> Color {
        if matches!(self, Self::Tmux { rgb: true, .. }) { Color::Rgb } else { Color::Indexed }
    }
    pub fn query(self, out: &mut Vec<u8>, bytes: &[u8]) {
        if !self.graphics() { return; }
        self.write_packet(out, bytes).expect("Vec writes cannot fail");
    }
    pub fn write_packet(self, out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Direct => out.write_all(bytes),
            Self::Tmux { passthrough: true, .. } => {
                out.write_all(b"\x1bPtmux;")?;
                for part in bytes.split_inclusive(|byte| *byte == 27) {
                    out.write_all(part)?;
                    if part.last() == Some(&27) { out.write_all(&[27])?; }
                }
                out.write_all(b"\x1b\\")
            }
            Self::Tmux { passthrough: false, .. } => Ok(()),
        }
    }
}
// Graphics helpers outlive individual frames but are all inside the terminal session.
static OUTPUT_TRANSPORT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub fn write_graphics(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let transport = match OUTPUT_TRANSPORT.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Transport::Tmux { passthrough: false, rgb: false },
        2 => Transport::Tmux { passthrough: true, rgb: false },
        _ => Transport::Direct,
    };
    write_graphics_with(transport, out, bytes)
}
fn write_graphics_with(transport: Transport, out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if transport == Transport::Direct { return out.write_all(bytes); }
    let mut rest = bytes;
    while let Some(start) = rest.windows(2).position(|p| p == b"\x1b_" || p == b"\x1bP") {
        out.write_all(&rest[..start])?;
        rest = &rest[start..];
        let Some(end) = rest.windows(2).position(|p| p == b"\x1b\\").map(|i| i + 2) else { return Ok(()); };
        transport.write_packet(out, &rest[..end])?;
        rest = &rest[end..];
    }
    out.write_all(rest)
}
fn parse(text: &str) -> Transport {
    let mut lines = text.lines();
    let passthrough = matches!(lines.next(), Some("on" | "all"));
    let rgb = lines.next().is_some_and(|line| line.split(',').any(|feature| feature == "RGB"));
    Transport::Tmux { passthrough, rgb }
}
fn discover_with(command: &mut Command, timeout: Duration) -> Transport {
    let fallback = Transport::Tmux { passthrough: false, rgb: false };
    let Ok(mut child) = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).process_group(0).spawn() else { return fallback; };
    let mut output = child.stdout.take().expect("piped stdout");
    const GET_FLAGS: i32 = 3;
    const SET_FLAGS: i32 = 4;
    extern "C" { fn fcntl(fd: i32, command: i32, ...) -> i32; }
    let flags = unsafe { fcntl(output.as_raw_fd(), GET_FLAGS) };
    let nonblocking = flags >= 0 && unsafe { fcntl(output.as_raw_fd(), SET_FLAGS, flags | crate::oflags::O_NONBLOCK) } >= 0;
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let result = loop {
        if !nonblocking || Instant::now() >= deadline { break fallback; }
        let mut buffer = [0; 1024];
        match output.read(&mut buffer) {
            Ok(n) if bytes.len() + n <= 4096 => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {},
            _ => break fallback,
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                // The pipe is nonblocking even when a descendant kept its write end.
                let mut tail = Vec::new();
                let _ = output.by_ref().take(4097).read_to_end(&mut tail);
                bytes.extend(tail);
                break if bytes.len() <= 4096 { std::str::from_utf8(&bytes).map(parse).unwrap_or(fallback) } else { fallback };
            }
            Ok(Some(_)) | Err(_) => break fallback,
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
        }
    };
    let _ = crate::procsignal::signal_group(child.id() as i32, 9);
    let _ = child.wait();
    result
}
#[cfg(test)]
#[path = "tmux_tests.rs"]
mod tests;
