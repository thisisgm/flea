//! Hyprland rounding is read off the UI thread at startup and on config reloads.
use flea::jsondoc::{self, Json};
use std::{
    io::{BufRead, BufReader},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub(crate) const CORNER_DEFAULT: f32 = 0.;

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

pub(crate) struct Service {
    stopped: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<UnixStream>>>,
}

impl Service {
    pub(crate) fn start(
        publish: impl FnMut(f32) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        let socket = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .zip(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|value| !value.is_empty()))
            .map(|(runtime, signature)| {
                PathBuf::from(runtime)
                    .join("hypr")
                    .join(signature)
                    .join(".socket2.sock")
            });
        Self::at(socket, PathBuf::from("hyprctl"), publish)
    }

    pub(crate) fn at(
        path: Option<PathBuf>,
        command: PathBuf,
        mut publish: impl FnMut(f32) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        let service = Self {
            stopped: Arc::new(AtomicBool::new(false)),
            socket: Arc::new(Mutex::new(None)),
        };
        let stopped = service.stopped.clone();
        let socket = service.socket.clone();
        std::thread::Builder::new()
            .name("flea-compositor".into())
            .spawn(move || {
                // Subscribe before reading so a reload racing the initial query is not lost.
                let stream = path.and_then(|path| UnixStream::connect(path).ok());
                {
                    let mut socket = socket.lock().unwrap();
                    if stopped.load(Ordering::Acquire) {
                        return;
                    }
                    *socket = stream.as_ref().and_then(|stream| stream.try_clone().ok());
                }
                if !publish(read(&mut Command::new(&command))) {
                    return;
                }
                let Some(stream) = stream else { return };
                for line in BufReader::new(stream).lines() {
                    if stopped.load(Ordering::Acquire) {
                        return;
                    }
                    let Ok(line) = line else { break };
                    if line
                        .split_once(">>")
                        .is_some_and(|(event, _)| event == "configreloaded")
                        && !publish(read(&mut Command::new(&command)))
                    {
                        return;
                    }
                }
                if !stopped.load(Ordering::Acquire) {
                    publish(CORNER_DEFAULT);
                }
            })?;
        Ok(service)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(socket) = self.socket.lock().unwrap().take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

#[cfg(test)]
#[path = "compositor_tests.rs"]
pub(crate) mod tests;
