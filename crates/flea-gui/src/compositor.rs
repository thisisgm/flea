//! Hyprland rounding is read off the UI thread at startup and on config reloads.
use flea::jsondoc::{self, Json};
use std::{
    io::{BufRead, BufReader},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Command,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

pub(crate) const CORNER_DEFAULT: f32 = 0.;
// The first wait before a lost or missing socket2 subscription is retried.
const RECONNECT_FIRST: Duration = Duration::from_millis(50);
// The longest wait between subscription retries.
const RECONNECT_CAP: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct Corner(pub f32);
impl gpui::Global for Corner {}

pub(crate) fn corner(cx: &gpui::App) -> f32 {
    cx.try_global::<Corner>()
        .map_or(CORNER_DEFAULT, |value| value.0)
}

pub(crate) fn valid_corner(value: f64) -> f32 {
    if value.is_finite() && (0. ..=f64::from(f32::MAX)).contains(&value) {
        value as f32
    } else {
        CORNER_DEFAULT
    }
}

// Sample input: {"option":"decoration:rounding","int":8,"set":true}.
fn rounding(answer: &str) -> f32 {
    jsondoc::parse(answer)
        .ok()
        .filter(|value| value.get("set").and_then(Json::as_bool) != Some(false))
        .and_then(|value| value.get("int").and_then(Json::as_f64))
        .map_or(CORNER_DEFAULT, valid_corner)
}

fn read(command: &mut Command) -> f32 {
    command
        .args(["getoption", "decoration:rounding", "-j"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map_or(CORNER_DEFAULT, |out| {
            rounding(&String::from_utf8_lossy(&out.stdout))
        })
}

// Hyprland's event socket for this session, absent outside one.
pub(crate) fn socket() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .zip(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|value| !value.is_empty()))
        .map(|(runtime, signature)| {
            PathBuf::from(runtime)
                .join("hypr")
                .join(signature)
                .join(".socket2.sock")
        })
}

#[derive(Default)]
struct Control {
    stopped: bool,
    socket: Option<UnixStream>,
}

pub(crate) struct Service(Arc<(Mutex<Control>, Condvar)>);

// Sample input: configreloaded>> followed by a newline; unrelated socket2 events are ignored.
fn reload(line: &[u8]) -> bool {
    line.strip_suffix(b"\n").unwrap_or(line) == b"configreloaded>>"
}

impl Service {
    pub(crate) fn start(
        publish: impl FnMut(f32) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::at(socket(), PathBuf::from("hyprctl"), publish)
    }

    pub(crate) fn at(
        path: Option<PathBuf>,
        command: PathBuf,
        mut publish: impl FnMut(f32) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        let service = Self(Arc::default());
        let control = service.0.clone();
        std::thread::Builder::new()
            .name("flea-compositor".into())
            .spawn(move || {
                let mut first = true;
                let mut backoff = RECONNECT_FIRST;
                loop {
                    // Subscribe before reading so a reload racing the query is not lost.
                    let stream = path
                        .as_ref()
                        .and_then(|path| UnixStream::connect(path).ok());
                    {
                        let mut state = control.0.lock().unwrap();
                        if state.stopped {
                            return;
                        }
                        state.socket = stream.as_ref().and_then(|stream| stream.try_clone().ok());
                    }
                    if (first || stream.is_some()) && !publish(read(&mut Command::new(&command))) {
                        return;
                    }
                    first = false;
                    if let Some(stream) = stream {
                        backoff = RECONNECT_FIRST;
                        let mut reader = BufReader::new(stream);
                        let mut line = Vec::new();
                        loop {
                            line.clear();
                            match reader.read_until(b'\n', &mut line) {
                                Ok(0) | Err(_) => break,
                                Ok(_) if reload(&line) => {
                                    // A reload still buffered after Drop is never acted on.
                                    if control.0.lock().unwrap().stopped
                                        || !publish(read(&mut Command::new(&command)))
                                    {
                                        return;
                                    }
                                }
                                Ok(_) => {}
                            }
                        }
                        let mut state = control.0.lock().unwrap();
                        state.socket = None;
                        if state.stopped {
                            return;
                        }
                        drop(state);
                        if !publish(CORNER_DEFAULT) {
                            return;
                        }
                    }
                    let Some(_) = path else { return };
                    // Retry the subscription, never poll the rounding option; Drop wakes this wait.
                    let (state, _) = control
                        .1
                        .wait_timeout_while(control.0.lock().unwrap(), backoff, |state| {
                            !state.stopped
                        })
                        .unwrap();
                    if state.stopped {
                        return;
                    }
                    backoff = (backoff * 2).min(RECONNECT_CAP);
                }
            })?;
        Ok(service)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let mut state = self.0 .0.lock().unwrap();
        state.stopped = true;
        if let Some(socket) = state.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        self.0 .1.notify_all();
    }
}

#[cfg(test)]
#[path = "compositor_tests.rs"]
pub(crate) mod tests;
