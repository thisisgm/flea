//! Native property changes feed the existing transport and metadata state.
use super::*;

pub(super) fn property(
    api: &Api,
    handle: Handle,
    event: &MpvEvent,
    state: &mut State,
    out: &mut Output,
) -> Result<(), String> {
    // SAFETY: PROPERTY_CHANGE carries an mpv_event_property.
    let changed = unsafe { &*(event.data as *const Property) };
    if event.userdata == VIDEO {
        // vid is unavailable for audio-only files; that is a valid false value.
        let id = if changed.data.is_null() {
            0
        } else {
            unsafe { *(changed.data as *const i64) }
        };
        if let Some(news) = state.metadata(VIDEO, id) {
            return out.event(news).map_err(|error| error.to_string());
        }
        return Ok(());
    }
    if changed.data.is_null() {
        return Ok(());
    }
    let io = |result: std::io::Result<()>| result.map_err(|error| error.to_string());
    // SAFETY: each id was observed with the format read here.
    let double = || unsafe { *(changed.data as *const f64) };
    let int = || unsafe { *(changed.data as *const i64) };
    match event.userdata {
        PAUSE => {
            // Queued flags can predate a completed seek; classify the current native state.
            if let Some(news) =
                get_flag(api, handle, "pause").and_then(|p| state.transport.paused(p))
            {
                io(out.event(news))?;
            }
        }
        TIME => {
            state.position_ms = ms(double());
            if (state.position_ms - state.sent_ms).abs() >= POSITION_STEP_MS {
                state.sent_ms = state.position_ms;
                io(out.event(Event::Position(state.position_ms)))?;
            }
        }
        DURATION => {
            if let Some(news) = state.metadata(DURATION, ms(double())) {
                io(out.event(news))?;
            }
        }
        EOF => {
            if let Some(news) =
                get_flag(api, handle, "eof-reached").and_then(|e| state.transport.ended(e))
            {
                io(out.event(news))?;
            }
        }
        WIDTH => state.clip.0 = int(),
        HEIGHT => state.clip.1 = int(),
        _ => {}
    }
    if matches!(event.userdata, WIDTH | HEIGHT) {
        let (w, h) = state.clip;
        state.size = decode_size(
            w.clamp(0, 1 << 16) as u32,
            h.clamp(0, 1 << 16) as u32,
            state.box_size.0,
            state.box_size.1,
        );
    }
    Ok(())
}
