//! The libmpv arm: one thread drains commands and events and renders BGRx frames into one reused buffer.
use super::mpv_api::{Event as MpvEvent, *};
use super::{opaque, Options, Output, INPUT};
use flea::ui_logic_pv3::frame::{Command, Event, Failure};
use flea::ui_logic_pv3::picture::decode_size;
use flea::ui_logic_pv3::transport::Transport;
use std::ffi::{c_int, c_void};
use std::ptr;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

// Observed property ids.
const PAUSE: u64 = 1;
const TIME: u64 = 2;
const DURATION: u64 = 3;
const EOF: u64 = 4;
const WIDTH: u64 = 5;
const HEIGHT: u64 = 6;
// The clock the strip draws moves in steps no finer than this.
const POSITION_STEP_MS: i64 = 100;

unsafe extern "C" fn wake(data: *mut c_void) {
    // SAFETY: data is the SyncSender `run` keeps alive until both callbacks are cleared.
    let sender = unsafe { &*(data as *const SyncSender<()>) };
    let _ = sender.try_send(());
}

/// What the loop knows about the open file.
struct State {
    transport: Transport,
    loaded: bool,
    video: bool,
    duration_ms: i64,
    position_ms: i64,
    sent_ms: i64,
    seeking: Option<i64>,
    clip: (i64, i64),
    size: (u32, u32),
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
    let mut state = State {
        transport: Transport::new(options.autoplay),
        loaded: false,
        video: false,
        duration_ms: 0,
        position_ms: 0,
        sent_ms: 0,
        seeking: None,
        clip: (0, 0),
        size: (0, 0),
    };
    let mut buffer: Vec<u8> = Vec::new();
    let io = |result: std::io::Result<()>| result.map_err(|error| error.to_string());
    loop {
        for command in commands.try_iter() {
            match command {
                Command::Play => {
                    if state.transport.play() {
                        seek(api, handle, &mut state, 0);
                    }
                    set(api, handle, "pause", "no")?;
                }
                Command::Pause => set(api, handle, "pause", "yes")?,
                Command::Stop => {
                    let stopped = state.transport.stop();
                    set(api, handle, "pause", "yes")?;
                    seek(api, handle, &mut state, 0);
                    // A stop's rewind is not a user seek, so it reports no Seeked.
                    state.seeking = None;
                    io(out.event(stopped))?;
                }
                Command::Seek(target) => seek(api, handle, &mut state, target),
                Command::Mute(muted) => set(api, handle, "mute", if muted { "yes" } else { "no" })?,
                // mpv's volume is cubic, so a linear gain g is 100 * cbrt(g).
                Command::Gain(gain) => {
                    set(api, handle, "volume", &format!("{:.3}", 100. * gain.cbrt()))?
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
                EVENT_END_FILE => {
                    // SAFETY: END_FILE carries an mpv_event_end_file.
                    let end = unsafe { &*(event.data as *const EndFile) };
                    if end.reason == END_FILE_ERROR {
                        io(out.event(Event::Failed(failure(end.error))))?;
                        return Ok(());
                    }
                }
                EVENT_FILE_LOADED => {
                    state.loaded = true;
                    state.duration_ms = get_double(api, handle, "duration").map_or(0, ms);
                    state.video = get_int(api, handle, "vid").is_some_and(|id| id > 0);
                    let duration_ms = state.duration_ms;
                    io(out.event(Event::Loaded {
                        video: state.video,
                        duration_ms,
                    }))?;
                    io(out.event(state.transport.loaded()))?;
                    if let Some(target) = state.transport.pending() {
                        seek(api, handle, &mut state, target);
                    }
                }
                EVENT_PLAYBACK_RESTART => {
                    if let Some(target) = state.seeking.take() {
                        io(out.event(Event::Seeked(target)))?;
                    }
                }
                EVENT_PROPERTY_CHANGE => property(event, &mut state, options, out)?,
                _ => {}
            }
        }
        // SAFETY: a live render context.
        let update = unsafe { (api.render_update)(context) };
        if update & RENDER_UPDATE_FRAME != 0 && state.size.0 > 0 && frame_due(api, context)? {
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
        Ok(()) => state.seeking = Some(target),
        Err(error) => eprintln!("flea-media: seek dropped: {error}"),
    }
}

fn property(
    event: &MpvEvent,
    state: &mut State,
    options: &Options,
    out: &mut Output,
) -> Result<(), String> {
    // SAFETY: PROPERTY_CHANGE carries an mpv_event_property.
    let changed = unsafe { &*(event.data as *const Property) };
    if changed.data.is_null() {
        return Ok(());
    }
    let io = |result: std::io::Result<()>| result.map_err(|error| error.to_string());
    // SAFETY: each id was observed with the format read here.
    let flag = || unsafe { *(changed.data as *const c_int) } != 0;
    let double = || unsafe { *(changed.data as *const f64) };
    let int = || unsafe { *(changed.data as *const i64) };
    match event.userdata {
        PAUSE => {
            if let Some(news) = state.transport.paused(flag()) {
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
            let duration_ms = ms(double());
            if state.loaded && duration_ms != state.duration_ms {
                state.duration_ms = duration_ms;
                io(out.event(Event::Loaded {
                    video: state.video,
                    duration_ms,
                }))?;
            }
        }
        EOF => {
            if let Some(news) = state.transport.ended(flag()) {
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
            options.box_w,
            options.box_h,
        );
    }
    Ok(())
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
