//! The tone-map filter is set, stepped down when play falls short under it, and dropped when mpv refuses it or the picture stalls.
use super::*;
use flea::ui_logic_media_preview::tonemap_pace::{process_cpu_ms, Look, Verdict};

/// Sets the tone-map filter; one mpv refuses at once is given up for the run, never the end of the helper.
pub(super) fn set_filter(api: &Api, handle: Handle, state: &mut State, filter: &str) {
    let shown = if filter.is_empty() { "off" } else { filter };
    // The chain is rebuilt, so frames count again only after a new settle.
    state.pace.restart();
    eprintln!("flea-media: tone_map={shown}");
    if let Err(error) = set(api, handle, "vf", filter) {
        drop_filter(api, handle, state, &error);
    }
    state.update_size();
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

/// Steps the mapped size down when play under the map presents too few of the clip's frames with the CPUs busy.
pub(super) fn pace(state: &mut State) {
    if !state.tonemap.on() {
        return;
    }
    // Without the CPU clock a busy machine cannot be told from a slow disk, so the size stays.
    let Some(cpu_ms) = process_cpu_ms() else {
        return;
    };
    let look = Look {
        wall_ms: state.origin.elapsed().as_millis() as u64,
        position_ms: state.position_ms,
        presented: state.presented,
        cpu_ms,
    };
    judge(state, look);
}

/// One look's verdict on the mapped size; the floor ends no judging, so a larger box later steps again.
pub(super) fn judge(state: &mut State, look: Look) {
    let rate = state.tonemap.rate_milli();
    match state.pace.observe(look, rate, state.cpus) {
        Verdict::Step(share) if state.tonemap.step(state.box_size.0, state.box_size.1) => {
            eprintln!(
                "flea-media: tone map stepped to level {} at {share}% of frames",
                state.tonemap.level()
            );
        }
        Verdict::Step(share) if state.floor_logged != Some(state.tonemap.level()) => {
            state.floor_logged = Some(state.tonemap.level());
            eprintln!("flea-media: tone map at its floor at {share}% of frames");
        }
        Verdict::Elsewhere(share) if !state.pace_noted => {
            state.pace_noted = true;
            eprintln!(
                "flea-media: tone map size kept at {share}% of frames: the CPUs are not busy"
            );
        }
        _ => {}
    }
}
