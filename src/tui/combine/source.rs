//! A held read-only source keeps dimensions and thumbnails on one file revision.
use super::*;
use crate::tui::job::{HeldSource, Job};
use std::{fs::Metadata, os::unix::fs::MetadataExt, time::Duration};

const THUMB_SIDE: usize = 256;
const THUMB_LIMIT: u64 = 1024 * 1024;
const SOURCE_ATTEMPTS: usize = 2;
const CANCEL_STEP: Duration = Duration::from_millis(20);
const REAP_WAIT: Duration = Duration::from_secs(12);

#[derive(PartialEq, Eq)]
struct Revision {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Revision {
    fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}
pub struct Source {
    pub path: String,
    pub size: (u32, u32),
    pub png: Vec<u8>,
}
pub fn load(paths: &[String], native: bool, cancel: &AtomicBool) -> Result<Vec<Source>, Text> {
    let mut sources = Vec::new();
    for path in paths {
        let mut loaded = None;
        for _ in 0..SOURCE_ATTEMPTS {
            if cancel.load(Ordering::Relaxed) {
                return Err(Text::Key("tui.combine.cancelled"));
            }
            let input =
                HeldSource::open(Path::new(path)).map_err(|e| crate::error::io_message(&e))?;
            let revision =
                Revision::of(&input.metadata().map_err(|e| crate::error::io_message(&e))?);
            let result = probe(path, native, &input, cancel);
            let unchanged = input.metadata().is_ok_and(|m| Revision::of(&m) == revision)
                && std::fs::symlink_metadata(path).is_ok_and(|m| Revision::of(&m) == revision);
            if unchanged {
                loaded = Some(result?);
                break;
            }
        }
        sources.push(loaded.ok_or(Text::Key("dialogs.output.error.dimensions"))?);
    }
    Ok(sources)
}
fn probe(
    path: &str,
    native: bool,
    input: &HeldSource,
    cancel: &AtomicBool,
) -> Result<Source, Text> {
    let info = decode(
        input,
        vec![
            "/usr/bin/magick".into(),
            "/input[0]".into(),
            "-auto-orient".into(),
            "-format".into(),
            "%w %h".into(),
            "info:".into(),
        ],
        cancel,
    )?;
    // ImageMagick dimensions sample: 800 600, the auto-oriented first frame of one held source.
    let values = std::str::from_utf8(&info)
        .map_err(|_| Text::Key("dialogs.output.error.dimensions"))?
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Text::Key("dialogs.output.error.dimensions"))?;
    if values.len() != 2 {
        return Err(Text::Key("dialogs.output.error.dimensions"));
    }
    // The card retains this held-source snapshot; external thumbnails lack full revision identity.
    let png = if native {
        decode(
            input,
            vec![
                "/usr/bin/magick".into(),
                "/input[0]".into(),
                "-auto-orient".into(),
                "-resize".into(),
                format!("{THUMB_SIDE}x{THUMB_SIDE}>"),
                "-strip".into(),
                "png:-".into(),
            ],
            cancel,
        )?
    } else {
        Vec::new()
    };
    if native && (png.is_empty() || png.len() as u64 > THUMB_LIMIT) {
        return Err(Text::Key("dialogs.output.error.dimensions"));
    }
    Ok(Source {
        path: path.into(),
        size: (values[0], values[1]),
        png,
    })
}
fn decode(input: &HeldSource, args: Vec<String>, cancel: &AtomicBool) -> Result<Vec<u8>, Text> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Text::Key("tui.combine.cancelled"));
    }
    let job = Job::start_held(input, args).map_err(|e| crate::error::io_message(&e))?;
    loop {
        match job.result.recv_timeout(CANCEL_STEP) {
            Ok(result) => return result.map_err(|diagnostic| job.failure(diagnostic)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Text::Key("dialogs.output.error.dimensions"))
            }
            Err(_) if cancel.load(Ordering::Relaxed) => {
                let result = crate::tui::graphics::cancel_job(job);
                let _ = result.recv_timeout(REAP_WAIT);
                return Err(Text::Key("tui.combine.cancelled"));
            }
            Err(_) => {}
        }
    }
}
