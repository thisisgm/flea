use super::{refused, validate, ColorMode, Grid, OUTPUT_LIMIT};
use crate::backend::{jail, regfile, sandbox};
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

struct Decoder {
    jailed: jail::Jailed,
    reader: Option<JoinHandle<io::Result<Vec<u8>>>>,
    finished: bool,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if !self.finished {
            jail::kill_tree(&mut self.jailed);
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub(super) fn run(
    input: File,
    executable: &Path,
    extent: (usize, usize),
    colors: ColorMode,
    cancel: &AtomicBool,
) -> io::Result<Grid> {
    if cancel.load(Ordering::Relaxed) {
        return Err(refused());
    }
    // Open the held inode read-only ourselves; bwrap must consume this descriptor before decoder exec.
    let readonly = regfile::open_if_regular(
        Path::new(&format!("/proc/self/fd/{}", input.as_raw_fd())),
        0,
    )?;
    drop(input);
    let cell_pixels = super::super::terminal::cell_pixels().unwrap_or((1, 2));
    let inner: Vec<String> = [
        executable.to_str().ok_or_else(refused)?.into(),
        "--format=symbols".into(),
        "--symbols=none,u2580,u2584".into(),
        "--fill=none".into(),
        "--optimize=0".into(),
        "--polite=on".into(),
        "--relative=off".into(),
        "--probe=off".into(),
        "--passthrough=none".into(),
        "--animate=off".into(),
        "--margin-bottom=0".into(),
        format!("--font-ratio={}/{}", cell_pixels.0, cell_pixels.1),
        format!(
            "--colors={}",
            if colors == ColorMode::Truecolor {
                "full"
            } else {
                "256"
            }
        ),
        format!("--size={}x{}", extent.0, extent.1),
        "--".into(),
        "/input".into(),
    ]
    .into();
    // Exactly the existing preview boundary, with a readonly executable bind for PATH entries outside /usr.
    let mut wrapped =
        sandbox::wrap_readonly_extra(&inner, &[executable, Path::new("/proc/self/fd/0")]);
    let destination = wrapped.len() - inner.len() - 1;
    wrapped[destination - 2] = "--ro-bind-fd".into();
    wrapped[destination - 1] = "0".into();
    wrapped[destination] = "/input".into();
    sandbox::add_status(&mut wrapped, inner.len());
    #[cfg(test)]
    super::test_support::host_loader(&mut wrapped);
    let jailed = jail::spawn_jailed(&wrapped, |cmd| {
        cmd.stdin(Stdio::from(readonly))
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
    })?;
    let mut decoder = Decoder {
        jailed,
        reader: None,
        finished: false,
    };
    let output = decoder.jailed.child.stdout.take().ok_or_else(refused)?;
    let overflow = Arc::new(AtomicBool::new(false));
    let exceeded = overflow.clone();
    decoder.reader = Some(
        std::thread::Builder::new()
            .name("flea-chafa-output".into())
            .spawn(move || {
                let mut bytes = Vec::new();
                output
                    .take(OUTPUT_LIMIT as u64 + 1)
                    .read_to_end(&mut bytes)?;
                exceeded.store(bytes.len() > OUTPUT_LIMIT, Ordering::Relaxed);
                Ok(bytes)
            })?,
    );
    let started = Instant::now();
    let mut status = None;
    loop {
        if cancel.load(Ordering::Relaxed)
            || overflow.load(Ordering::Relaxed)
            || started.elapsed() >= DEADLINE
        {
            return Err(refused());
        }
        if status.is_none() {
            status = decoder.jailed.child.try_wait()?;
        }
        if status.is_some() && decoder.reader.as_ref().unwrap().is_finished() {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let bytes = decoder
        .reader
        .take()
        .unwrap()
        .join()
        .map_err(|_| refused())??;
    decoder.finished = true;
    if !status.unwrap().success() {
        return Err(refused());
    }
    validate(&bytes, extent.0, extent.1)
}
