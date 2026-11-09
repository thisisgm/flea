//! The libmpv arm: one thread drains commands and events and renders BGRx frames into one reused buffer.
use super::mpv_api::{Event as MpvEvent, *};
use super::{opaque, Options, Output, INPUT};
use flea::ui_logic_media_preview::frame::{Command, Event, Failure};
use flea::ui_logic_media_preview::picture::decode_size;
use flea::ui_logic_media_preview::tonemap::stall_due;
use std::ffi::{c_int, c_void};
use std::ptr;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

#[path = "mpv_state.rs"]
mod state;
use state::State;

#[path = "mpv_property.rs"]
mod properties;
use properties::property;

#[path = "mpv_filter.rs"]
mod filter;
use filter::{drop_filter, moving, set_filter};

#[cfg(test)]
#[path = "mpv_tests.rs"]
mod tests;

// Observed property ids.
const PAUSE: u64 = 1;
const TIME: u64 = 2;
const DURATION: u64 = 3;
const EOF: u64 = 4;
const WIDTH: u64 = 5;
const HEIGHT: u64 = 6;
const VIDEO: u64 = 7;
const GAMMA: u64 = 8;
const SOURCE_WIDTH: u64 = 9;
const SOURCE_HEIGHT: u64 = 10;
const ROTATE: u64 = 11;
// The clock the strip draws moves in steps no finer than this.
const POSITION_STEP_MS: i64 = 100;
// A frame released before the clip's size is known renders into a 2x2 RGBA scrap nobody shows.
const SCRAP_SIDE: u32 = 2;
const SCRAP_STRIDE: usize = SCRAP_SIDE as usize * 4;
// A size mpv reports is clamped to this side before it can size a frame buffer.
const MAX_SIDE: i64 = 1 << 16;

unsafe extern "C" fn wake(data: *mut c_void) {
    // SAFETY: data is the SyncSender `run` keeps alive until both callbacks are cleared.
    let sender = unsafe { &*(data as *const SyncSender<()>) };
    let _ = sender.try_send(());
}

fn ms(seconds: f64) -> i64 {
    (seconds * 1000.).round().max(0.) as i64
}

fn failure(error: c_int) -> Failure {
    match error {
        ERROR_NOTHING_TO_PLAY | ERROR_UNKNOWN_FORMAT => Failure::Unsupported,
        _ => Failure::Unreadable,
    }
}

pub fn run(
    options: &Options,
    out: &mut Output,
    commands: &Receiver<Command>,
) -> Result<(), String> {
    let api = Api::load()?;
    // SAFETY: mpv_create has no preconditions; the handle is checked.
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return Err("mpv_create failed".into());
    }
    let pause = if options.autoplay { "no" } else { "yes" };
    let audio = if options.audio { "pulse" } else { "null" };
    for (name, value) in [
        ("config", "no"),
        ("load-scripts", "no"),
        ("terminal", "no"),
        ("input-default-bindings", "no"),
        ("audio-file-auto", "no"),
        ("audio-display", "no"),
        ("sub-auto", "no"),
        ("ytdl", "no"),
        ("hwdec", "no"),
        ("vo", "libmpv"),
        ("ao", audio),
        ("keep-open", "yes"),
        ("pause", pause),
    ] {
        let (name, value) = (text(name), text(value));
        // SAFETY: options are set before initialize, as the API requires.
        check(
            unsafe { (api.set_option)(handle, name.as_ptr(), value.as_ptr()) },
            "option",
        )?;
    }
    // SAFETY: a created, not yet initialized handle.
    check(unsafe { (api.initialize)(handle) }, "initialize")?;
    let (notify, notified) = mpsc::sync_channel::<()>(1);
    let notify = Box::new(notify);
    let notify_ptr = (&*notify as *const SyncSender<()>).cast_mut().cast();
    let mut context: Handle = ptr::null_mut();
    let mut parameters = [
        Param {
            kind: RENDER_API_TYPE,
            data: c"sw".as_ptr().cast_mut().cast(),
        },
        Param {
            kind: 0,
            data: ptr::null_mut(),
        },
    ];
    // SAFETY: an initialized handle and a terminated parameter list.
    let created = unsafe { (api.render_create)(&mut context, handle, parameters.as_mut_ptr()) };
    let result = check(created, "render create").and_then(|()| {
        // SAFETY: the sender outlives both callbacks, which are cleared before it drops.
        unsafe {
            (api.render_callback)(context, Some(wake), notify_ptr);
            (api.set_wakeup)(handle, Some(wake), notify_ptr);
        }
        // eof-reached is observed first: at the end libmpv then reports it before the pause keep-open makes.
        for (id, name, format) in [
            (EOF, "eof-reached", FORMAT_FLAG),
            (PAUSE, "pause", FORMAT_FLAG),
            (TIME, "time-pos", FORMAT_DOUBLE),
            (DURATION, "duration", FORMAT_DOUBLE),
            (WIDTH, "dwidth", FORMAT_INT64),
            (HEIGHT, "dheight", FORMAT_INT64),
            (VIDEO, "vid", FORMAT_INT64),
            // The decoder's own output, which the tone-map filter never changes.
            (GAMMA, "video-dec-params/gamma", FORMAT_STRING),
            (SOURCE_WIDTH, "video-dec-params/dw", FORMAT_INT64),
            (SOURCE_HEIGHT, "video-dec-params/dh", FORMAT_INT64),
            (ROTATE, "video-dec-params/rotate", FORMAT_INT64),
        ] {
            let name = text(name);
            // SAFETY: a live handle and a NUL-terminated name.
            check(
                unsafe { (api.observe)(handle, id, name.as_ptr(), format) },
                "observe",
            )?;
        }
        command(&api, handle, &["loadfile", INPUT])?;
        play(&api, handle, context, options, out, commands, &notified)
    });
    // SAFETY: callbacks are cleared and the render context freed before the core is destroyed.
    unsafe {
        if !context.is_null() {
            (api.render_callback)(context, None, ptr::null_mut());
            (api.render_free)(context);
        }
        (api.set_wakeup)(handle, None, ptr::null_mut());
        (api.terminate)(handle);
    }
    drop(notify);
    result
}

fn play(
    api: &Api,
    handle: Handle,
    context: Handle,
    options: &Options,
    out: &mut Output,
    commands: &Receiver<Command>,
    notified: &Receiver<()>,
) -> Result<(), String> {
    let mut state = State::new(options.autoplay);
    let mut buffer: Vec<u8> = Vec::new();
    // A frame released unseen while its size was unknown; it is drawn again once the size is known.
    let mut owed = false;
    state.box_size = (options.box_w, options.box_h);
    let mut redraw = false;
    // The last frame, command, load, restart or pause change: frames are owed only after it.
    let mut progress = Instant::now();
    let io = |result: std::io::Result<()>| result.map_err(|error| error.to_string());
    loop {
        for command in commands.try_iter() {
            progress = Instant::now();
            match command {
                Command::Play => {
                    if state.transport.play() {
                        seek(api, handle, &mut state, 0);
                    }
                    set(api, handle, "pause", "no")?;
                }
                Command::Pause => {
                    state.transport.pause();
                    set(api, handle, "pause", "yes")?;
                }
                Command::Stop => {
                    set(api, handle, "pause", "yes")?;
                    seek(api, handle, &mut state, 0);
                    // A stop's rewind is not a user seek, so it reports no Seeked.
                    state.seeking = None;
                    io(out.event(state.transport.stop()))?;
                }
                Command::Seek(target) => seek(api, handle, &mut state, target),
                Command::Mute(muted) => set(api, handle, "mute", if muted { "yes" } else { "no" })?,
                // mpv's volume is cubic, so a linear gain g is 100 * cbrt(g).
                Command::Gain(gain) => {
                    set(api, handle, "volume", &format!("{:.3}", 100. * gain.cbrt()))?
                }
                Command::Resize(w, h) => {
                    state.box_size = (w, h);
                    state.size = decode_size(
                        state.clip.0.clamp(0, MAX_SIDE) as u32,
                        state.clip.1.clamp(0, MAX_SIDE) as u32,
                        w,
                        h,
                    );
                    redraw = true;
                }
                Command::Quit => return Ok(()),
            }
        }
        loop {
            // SAFETY: a live handle; a zero timeout never blocks.
            let event = unsafe { &*(api.wait_event)(handle, 0.) };
            match event.id {
                EVENT_NONE => break,
                EVENT_SHUTDOWN => return Ok(()),
                EVENT_COMMAND_REPLY => {
                    if event.userdata == SEEK_REPLY {
                        if let Err(error) = check(event.error, "seek") {
                            state.seeking = None;
                            eprintln!("flea-media: seek dropped: {error}");
                        }
                    } else if event.userdata == FILTER_REPLY {
                        if let Err(error) = check(event.error, "vf") {
                            drop_filter(api, handle, &mut state, &error);
                        }
                    } else {
                        check(event.error, "command reply")?;
                    }
                }
                EVENT_END_FILE => {
                    // SAFETY: END_FILE carries an mpv_event_end_file.
                    let end = unsafe { &*(event.data as *const EndFile) };
                    if end.reason == END_FILE_ERROR {
                        io(out.event(Event::Failed(failure(end.error))))?;
                        return Ok(());
                    }
                }
                EVENT_FILE_LOADED => {
                    progress = Instant::now();
                    // Observers supply metadata without a synchronous core call on the render thread.
                    state.loaded = true;
                    io(out.event(state.facts()))?;
                    io(out.event(state.transport.loaded()))?;
                    if let Some(target) = state.transport.pending() {
                        seek(api, handle, &mut state, target);
                    }
                }
                EVENT_PLAYBACK_RESTART => {
                    progress = Instant::now();
                    if let Some(target) = state.seeking {
                        if let Some((paused, eof, at_end)) = completed_seek(api, handle, target) {
                            state.seeking = None;
                            io(out.event(Event::Seeked(target)))?;
                            if let Some(news) = state.transport.seek_completed(paused, eof, at_end)
                            {
                                io(out.event(news))?;
                            }
                        }
                    }
                }
                EVENT_PROPERTY_CHANGE => {
                    if event.userdata == PAUSE {
                        progress = Instant::now();
                    }
                    property(api, handle, event, &mut state, out)?
                }
                _ => {}
            }
        }
        if let Some(filter) = state.tonemap.update(state.box_size.0, state.box_size.1) {
            progress = Instant::now();
            set_filter(api, handle, &mut state, &filter);
        }
        if stall_due(state.tonemap.on(), progress.elapsed()) {
            progress = Instant::now();
            if moving(api, handle) {
                drop_filter(api, handle, &mut state, "no frame came");
            }
        }
        // SAFETY: a live render context.
        let update = unsafe { (api.render_update)(context) };
        let pending = update & RENDER_UPDATE_FRAME != 0;
        let draw = super::draw(pending, state.size.0 > 0, owed || redraw);
        if draw != super::Draw::Nothing {
            progress = Instant::now();
        }
        if draw == super::Draw::Release {
            let mut scrap = [0_u8; SCRAP_STRIDE * SCRAP_SIDE as usize];
            render(
                api,
                context,
                SCRAP_SIDE,
                SCRAP_SIDE,
                SCRAP_STRIDE,
                &mut scrap,
            )?;
            owed = true;
        }
        if draw == super::Draw::Frame && (owed || redraw || frame_due(api, context)?) {
            owed = false;
            redraw = false;
            let (width, height) = state.size;
            let stride = width as usize * 4;
            buffer.resize(stride * height as usize, 0);
            render(api, context, width, height, stride, &mut buffer)?;
            opaque(&mut buffer);
            io(out.frame(width, height, &buffer))?;
            continue;
        }
        let _ = notified.recv_timeout(Duration::from_millis(10));
    }
}

/// Seeks now, or once the file loads; a seek mpv refuses is dropped as Qt drops it, never the end of the helper.
fn seek(api: &Api, handle: Handle, state: &mut State, target: i64) {
    let Some(target) = state.transport.seek(target) else {
        return;
    };
    let seconds = format!("{:.3}", target.max(0) as f64 / 1000.);
    match command(api, handle, &["seek", &seconds, "absolute+exact"]) {
        Ok(()) => {
            state.transport.seek_accepted();
            state.seeking = Some(target);
        }
        Err(error) => eprintln!("flea-media: seek dropped: {error}"),
    }
}

fn frame_due(api: &Api, context: Handle) -> Result<bool, String> {
    let mut info = FrameInfo {
        flags: 0,
        target_time: 0,
    };
    let parameter = Param {
        kind: RENDER_NEXT_FRAME_INFO,
        data: (&mut info as *mut FrameInfo).cast(),
    };
    // SAFETY: NEXT_FRAME_INFO fills an mpv_render_frame_info.
    check(
        unsafe { (api.render_info)(context, parameter) },
        "frame info",
    )?;
    Ok(info.flags & FRAME_INFO_PRESENT != 0)
}

fn render(
    api: &Api,
    context: Handle,
    width: u32,
    height: u32,
    stride: usize,
    buffer: &mut [u8],
) -> Result<(), String> {
    let mut size = [width as c_int, height as c_int];
    let mut stride = stride;
    let mut parameters = [
        Param {
            kind: RENDER_SW_SIZE,
            data: size.as_mut_ptr().cast(),
        },
        Param {
            kind: RENDER_SW_FORMAT,
            data: c"bgr0".as_ptr().cast_mut().cast(),
        },
        Param {
            kind: RENDER_SW_STRIDE,
            data: (&mut stride as *mut usize).cast(),
        },
        Param {
            kind: RENDER_SW_POINTER,
            data: buffer.as_mut_ptr().cast(),
        },
        Param {
            kind: 0,
            data: ptr::null_mut(),
        },
    ];
    // SAFETY: the buffer holds stride * height bytes and outlives the call.
    check(
        unsafe { (api.render)(context, parameters.as_mut_ptr()) },
        "render",
    )
}
