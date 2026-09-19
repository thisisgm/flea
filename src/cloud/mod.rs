// One bounded, read-only probe. The UI owns polling; local paths cause no RC requests.
mod config;
mod http;
mod state;

use crate::backend::mountinfo::mount_in;
use crate::jsondoc::Json;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub fn run(path: PathBuf) -> i32 {
    let display = path.to_string_lossy().to_string();
    let (tx, rx) = mpsc::sync_channel(1);
    // The process exits after this deadline even if connect() or a config read is wedged.
    std::thread::spawn(move || { let _ = tx.send(inspect(&path)); });
    let mut result = rx.recv_timeout(Duration::from_secs(4))
        .unwrap_or_else(|error| state::Snapshot::unavailable(match error {
            mpsc::RecvTimeoutError::Timeout => "Status query timed out",
            mpsc::RecvTimeoutError::Disconnected => "Status probe failed",
        }));
    result.path = display;
    println!("{}", result.json());
    0
}

fn inspect(path: &Path) -> state::Snapshot {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return state::Snapshot::unavailable("An absolute path without '..' is required");
    }
    let body = match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(body) => body,
        Err(_) => return state::Snapshot::unavailable("Mount information is unavailable"),
    };
    let Some(mount) = mount_in(path, &body) else { return state::Snapshot::new("local"); };
    if mount.kind != "fuse.rclone" { return state::Snapshot::new("local"); }
    let mut result = match config::socket_for(&mount.path) {
        Ok(Some(socket)) => probe(&socket, &mount.source),
        Ok(None) => state::Snapshot::unavailable("Upload monitoring is not configured"),
        Err(reason) => state::Snapshot::unavailable(reason),
    };
    result.mount = mount.path.to_string_lossy().to_string();
    result
}

fn probe(socket: &Path, source: &str) -> state::Snapshot {
    fn read(socket: &Path, source: &str) -> Result<state::Snapshot, &'static str> {
        config::private_socket(socket)?;
        let body = format!(r#"{{"fs":"{}"}}"#, crate::json::escape(source));
        let stats = http::post(socket, "vfs/stats", &body)?;
        if stats.get("fs").and_then(Json::as_str) != Some(source) {
            return Err("Status endpoint does not match this mount");
        }
        let queue = http::post(socket, "vfs/queue", &body)?;
        let summary = state::summarize(&stats, &queue, None)?;
        if summary.uploading == 0 { return Ok(summary); }
        // Global transfer samples are useful only for a single-VFS endpoint, and remain optional.
        let vfses = http::post(socket, "vfs/list", "{}").ok();
        let single = vfses.as_ref().and_then(|v| v.get("vfses")).and_then(Json::as_array)
            .is_some_and(|v| v.len() == 1 && v[0].as_str() == Some(source));
        let transfers = if single { http::post(socket, "core/stats", "{}").ok() } else { None };
        state::summarize(&stats, &queue, transfers.as_ref())
    }
    read(socket, source).unwrap_or_else(state::Snapshot::unavailable)
}
