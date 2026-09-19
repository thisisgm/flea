mod config;
mod lock;
mod runner;
mod source;
use crate::json::escape;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

pub fn emit(state: &str, message: &str, bytes: f64, total: f64, speed: f64) {
    println!(
        r#"{{"state":"{}","message":"{}","bytes":{},"total":{},"speed":{}}}"#,
        escape(state),
        escape(message),
        bytes,
        total,
        speed
    );
    let _ = std::io::stdout().flush();
}
pub fn targets() -> i32 {
    match config::load() {
        Ok(targets) => {
            let entries = targets
                .iter()
                .map(|t| {
                    format!(
                        r#"{{"id":"{}","label":"{}","destination":"{}:{}"}}"#,
                        escape(&t.id),
                        escape(&t.label),
                        escape(&t.remote),
                        escape(&t.root)
                    )
                })
                .collect::<Vec<_>>();
            println!(r#"{{"targets":[{}]}}"#, entries.join(","));
            0
        }
        Err(error) => {
            emit("error", &error, 0.0, 0.0, 0.0);
            1
        }
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let base = std::env::temp_dir();
        for n in 0..100 {
            let path = base.join(format!("flea-cloud-copy-{}-{n}", std::process::id()));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("Cannot create private upload workspace".into()),
            }
        }
        Err("Cannot allocate upload workspace".into())
    }
    fn write(&self, name: &str, body: &str) -> Result<String, String> {
        let path = self.0.join(name);
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|_| "Cannot create upload manifest")?;
        file.write_all(body.as_bytes())
            .map_err(|_| "Cannot write upload manifest")?;
        Ok(path.to_string_lossy().into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).into()).collect()
}
fn upload(id: &str, folder: &str, path: &Path, run: &runner::Runner) -> Result<(), String> {
    let target = config::load()?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("Cloud target no longer configured")?;
    let _lock = lock::acquire(&target.remote)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Source has no filename")?;
    let (parent, dest) = config::destination(&target, folder, name)?;
    emit(
        "preparing",
        "Preparing checksums; originals will be kept.",
        0.0,
        0.0,
        0.0,
    );
    let (directory, before) = source::inspect(path, run)?;
    let scratch = Scratch::new()?;
    let list = scratch.write(
        "files",
        &before.keys().map(|p| format!("{p}\n")).collect::<String>(),
    )?;
    let src = path.to_str().ok_or("Source is not UTF-8")?;
    let mut hashing = args(&["md5sum", src]);
    if directory {
        hashing.extend(args(&["--files-from-raw", &list]));
    }
    let hash = run.run(&hashing, "preparing")?;
    source::manifest(&hash, &before)?;
    let manifest = scratch.write("md5", &hash)?;
    if source::inspect(path, run)?.1 != before {
        return Err("Source changed while preparing. Retry when edits have finished.".into());
    }
    emit(
        "uploading",
        "Copying directly to the cloud; existing different files are refused.",
        0.0,
        0.0,
        0.0,
    );
    let mut copy = args(&[
        "copy",
        src,
        if directory { &dest } else { &parent },
        "--immutable",
        "--ignore-existing",
        "--checksum",
    ]);
    if directory {
        copy.extend(args(&["--files-from-raw", &list]));
    }
    run.run(&copy, "uploading")?;
    emit(
        "verifying",
        "Checking remote MD5 checksums against the prepared manifest.",
        0.0,
        0.0,
        0.0,
    );
    let checked = run.run(
        &args(&[
            "checksum",
            "MD5",
            &manifest,
            if directory { &dest } else { &parent },
            "--one-way",
            "--match",
            "-",
        ]),
        "verifying",
    )?;
    let matches = checked.lines().collect::<std::collections::BTreeSet<_>>();
    if matches.len() != before.len() || !before.keys().all(|p| matches.contains(p.as_str())) {
        return Err(
            "Remote checksums did not positively confirm every file. Originals kept.".into(),
        );
    }
    if source::inspect(path, run)?.1 != before {
        return Err("Source changed during upload. The prepared copy was checked, but current originals are not confirmed.".into());
    }
    run.check()?;
    emit(
        "done",
        "Upload verified with MD5. Originals kept. Empty directories are not copied.",
        0.0,
        0.0,
        0.0,
    );
    Ok(())
}
pub fn run(id: &str, folder: &str, source: &str) -> i32 {
    if let Err(error) = runner::owner_lifetime() {
        emit("error", &error, 0.0, 0.0, 0.0);
        return 1;
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let input = cancelled.clone();
    // EOF (window closed) and any input (Cancel) both stop only this job's child.
    std::thread::spawn(move || {
        let _ = std::io::stdin().read(&mut [0u8; 1]);
        input.store(true, Ordering::Relaxed);
    });
    let run = runner::Runner {
        cancel: cancelled.clone(),
        deadline: Instant::now() + Duration::from_secs(24 * 3600),
    };
    match upload(id, folder, Path::new(source), &run) {
        Ok(()) => 0,
        Err(error) => {
            emit(
                if cancelled.load(Ordering::Relaxed) {
                    "cancelled"
                } else {
                    "error"
                },
                &error,
                0.0,
                0.0,
                0.0,
            );
            1
        }
    }
}
