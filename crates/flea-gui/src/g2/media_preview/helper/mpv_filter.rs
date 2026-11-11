//! The tone-map filter is set, and dropped when mpv refuses it or the picture stalls.
use super::*;

/// Sets the tone-map filter; one mpv refuses at once is given up for the run, never the end of the helper.
pub(super) fn set_filter(api: &Api, handle: Handle, state: &mut State, filter: &str) {
    let shown = if filter.is_empty() { "off" } else { filter };
    eprintln!("flea-media: tone_map={shown}");
    if let Err(error) = set(api, handle, "vf", filter) {
        drop_filter(api, handle, state, &error);
    }
}

/// The filter was refused or stalled the picture: the renderer converts again, as it did before the filter.
pub(super) fn drop_filter(api: &Api, handle: Handle, state: &mut State, why: &str) {
    eprintln!("flea-media: tone map dropped: {why}");
    if let Some(off) = state.tonemap.fail() {
        set_filter(api, handle, state, &off);
    }
}

/// Play should be producing frames: not paused, seeking, waiting on the cache or parked at the end.
pub(super) fn moving(api: &Api, handle: Handle) -> bool {
    ["pause", "seeking", "paused-for-cache", "eof-reached"]
        .into_iter()
        .all(|name| get_flag(api, handle, name) == Some(false))
}
