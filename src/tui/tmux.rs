use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DISCOVERY_OUTPUT_LIMIT: usize = 4096;
const SIGKILL: i32 = 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transport {
    #[default]
    Direct,
    Tmux { passthrough: bool },
}
impl Transport {
    pub fn discover() -> Self {
        if std::env::var_os("TMUX").is_none_or(|v| v.is_empty()) { return Self::Direct; }
        let mut command = Command::new("tmux");
        command.args(["display-message", "-p"]);
        if let Some(pane) = std::env::var_os("TMUX_PANE").filter(|v| !v.is_empty()) { command.arg("-t").arg(pane); }
        command.arg("#{allow-passthrough}");
        discover_with(&mut command, Duration::from_millis(150))
    }
    pub fn publish(self) {
        OUTPUT_TRANSPORT.store(match self { Self::Direct => 0, Self::Tmux { passthrough: false, .. } => 1, Self::Tmux { passthrough: true, .. } => 2 }, std::sync::atomic::Ordering::Relaxed);
    }
    pub fn graphics(self) -> bool {
        !matches!(self, Self::Tmux { passthrough: false, .. })
    }
    pub fn query(self, out: &mut Vec<u8>, bytes: &[u8]) {
        if !self.graphics() && bytes != b"\x1b[c" { return; }
        self.write_packet(out, bytes).expect("Vec writes cannot fail");
    }
    pub fn write_packet(self, out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
        if self != Self::Direct && (bytes.starts_with(b"\x1b_") || bytes.starts_with(b"\x1bP")) && !bytes.ends_with(b"\x1b\\") {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, super::words::Text::Key("tui.tmux.graphics_packet_has_no_string_terminator")));
        }
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
            Self::Tmux { passthrough: false, .. } => {
                if bytes == b"\x1b[c" || is_sixel(bytes) { out.write_all(bytes) } else { Ok(()) }
            }
        }
    }
}
// Graphics helpers outlive individual frames but are all inside the terminal session.
static OUTPUT_TRANSPORT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub fn write_graphics(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let transport = match OUTPUT_TRANSPORT.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Transport::Tmux { passthrough: false },
        2 => Transport::Tmux { passthrough: true },
        _ => Transport::Direct,
    };
    write_graphics_with(transport, out, bytes)
}
// Sample input: ESC[s ESC_Ga=t;AAAA ST ESC[u wraps the APC and leaves cursor controls intact.
fn write_graphics_with(transport: Transport, out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if transport == Transport::Direct { return out.write_all(bytes); }
    let mut rest = bytes;
    while let Some(start) = rest.windows(2).position(|p| p == b"\x1b_" || p == b"\x1bP") {
        out.write_all(&rest[..start])?;
        rest = &rest[start..];
        let end = rest.windows(2).position(|p| p == b"\x1b\\").map(|i| i + 2)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, super::words::Text::Key("tui.tmux.graphics_packet_has_no_string_terminator")))?;
        transport.write_packet(out, &rest[..end])?;
        rest = &rest[end..];
    }
    out.write_all(rest)
}
// Sample input: ESC P0;1;0q#1~ ST is sixel; ESC P+q524742 ST is a different DCS.
fn is_sixel(bytes: &[u8]) -> bool {
    bytes.strip_prefix(b"\x1bP").and_then(|body| body.iter().find(|b| !b.is_ascii_digit() && **b != b';')) == Some(&b'q')
}
// Sample input: on\n or all\n enables passthrough; off\n keeps tmux's own protocols.
fn parse(text: &str) -> Transport {
    let passthrough = matches!(text.trim(), "on" | "all");
    Transport::Tmux { passthrough }
}
fn discover_with(command: &mut Command, timeout: Duration) -> Transport {
    let fallback = Transport::Tmux { passthrough: false };
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
            Ok(n) if bytes.len() + n <= DISCOVERY_OUTPUT_LIMIT => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {},
            _ => break fallback,
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                // The pipe is nonblocking even when a descendant kept its write end.
                let mut tail = Vec::new();
                let _ = output.by_ref().take((DISCOVERY_OUTPUT_LIMIT + 1) as u64).read_to_end(&mut tail);
                bytes.extend(tail);
                break if bytes.len() <= DISCOVERY_OUTPUT_LIMIT { std::str::from_utf8(&bytes).map(parse).unwrap_or(fallback) } else { fallback };
            }
            Ok(Some(_)) | Err(_) => break fallback,
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
        }
    };
    let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
    let _ = child.wait();
    result
}
#[cfg(test)]
#[path = "tmux_tests.rs"]
mod tests;
