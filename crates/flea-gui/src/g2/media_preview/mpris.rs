//! One player's MPRIS bridge: the flea-mpris helper, started on the first play and ended with the player.
use flea::ui_logic_media_preview::session::Remote;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;

/// The helper an installed Flea ships, unless the environment names another.
pub const HELPER: &str = "/usr/lib/flea/flea-mpris";
pub const HELPER_ENV: &str = "FLEA_MPRIS";

pub fn program(value: Option<OsString>) -> PathBuf {
    value
        .filter(|value| !value.is_empty())
        .map_or_else(|| HELPER.into(), PathBuf::from)
}

/// The UI's end: state lines go out, remote commands come back; dropping it kills the helper.
pub struct Mpris {
    lines: mpsc::Sender<String>,
}

impl Mpris {
    /// Starts the helper off the UI thread; `first` is the state line it is given once it is up.
    pub fn start(program: PathBuf, first: String, remote: async_channel::Sender<Remote>) -> Self {
        let (lines, received) = mpsc::channel::<String>();
        let _ = lines.send(first);
        let _ = std::thread::Builder::new()
            .name("flea-mpris".into())
            .spawn(move || run(program, &received, remote));
        Self { lines }
    }

    pub fn send(&self, line: String) {
        let _ = self.lines.send(line);
    }
}

fn run(program: PathBuf, lines: &mpsc::Receiver<String>, remote: async_channel::Sender<Remote>) {
    let spawned = Command::new(&program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            eprintln!("flea-gui: {}: {error}", program.display());
            return;
        }
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    let reader = std::thread::Builder::new()
        .name("flea-mpris-read".into())
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(command) = Remote::parse(line.trim_end()) {
                    if remote.send_blocking(command).is_err() {
                        break;
                    }
                }
            }
        });
    for line in lines {
        if writeln!(stdin, "{line}").is_err() {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(reader) = reader {
        let _ = reader.join();
    }
}
