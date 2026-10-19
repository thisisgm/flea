//! Checked rclone send (#176): requests, replies, per-send state and words, shared by every front end.
//! The wire is the backend's "Checked rclone sends (0.4.0)" and "Jobs and Actions (0.4.0)" contract in docs/protocol.md.
use crate::json::{escape, string_array};

pub mod present;
pub mod send;
#[cfg(test)]
mod tests;
pub mod wire;

pub use send::{Phase, Send};
pub use wire::{Event, Job, Outcome};

/// The row's storage id in Settings > Menus `menu.hidden`; absent from that list means the row is on.
pub const MENU_ID: &str = "sendRclone";
/// One check sweep, start to start; the segment is a third of the track.
pub const SWEEP_PERIOD_MS: u64 = 1400;
pub const SWEEP_SHARE: f32 = 1. / 3.;

/// Why a folder typed into the send card cannot be sent to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderRefusal {
    Empty,
    Invalid,
}

/// Sample input: "gdrive", "my drive"; the backend's rule, so no name can carry a connection string or a path.
pub fn remote_name_ok(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '/' | '\\' | ','))
}

/// Sample input: "Uploads" or "Photos/2025"; empty dims Send as Copy To does, and a `..` component or a control character is refused before any request.
pub fn folder_check(folder: &str) -> Result<&str, FolderRefusal> {
    if folder.is_empty() {
        return Err(FolderRefusal::Empty);
    }
    if folder.chars().any(char::is_control) || folder.split('/').any(|part| part == "..") {
        return Err(FolderRefusal::Invalid);
    }
    Ok(folder)
}

pub fn remotes_line(id: u64) -> String {
    format!(r#"{{"c":"rclone","op":"remotes","id":{id}}}"#)
}

/// Paths are absolute and resolved once by the caller from one listing.
pub fn send_line(id: u64, remote: &str, folder: &str, paths: &[String]) -> String {
    format!(
        r#"{{"c":"rclone","op":"send","id":{id},"remote":"{}","folder":"{}","paths":{}}}"#,
        escape(remote),
        escape(folder),
        string_array(paths)
    )
}

/// Cancel, Retry and Dismiss are the ordinary jobs requests, by the backend's job id.
pub fn jobs_line(op: &str, job: u64) -> String {
    format!(r#"{{"c":"jobs","op":"{}","id":{job}}}"#, escape(op))
}

/// The check bar's accent segment at this many milliseconds into the check, as (left, width) shares of the track, clipped to it.
pub fn sweep(elapsed_ms: u64) -> (f32, f32) {
    let phase = (elapsed_ms % SWEEP_PERIOD_MS) as f32 / SWEEP_PERIOD_MS as f32;
    // OutCubic eases each pass, as Omarchy's motion does, so the segment slows into the far edge.
    let eased = 1. - (1. - phase).powi(3);
    let left = eased * (1. + SWEEP_SHARE) - SWEEP_SHARE;
    let start = left.max(0.);
    let end = (left + SWEEP_SHARE).min(1.);
    (start, (end - start).max(0.))
}
