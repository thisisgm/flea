//! libmpv's client and software render API, the subset the helper calls (mpv/client.h, mpv/render.h).
use super::dl::{symbols, Library};
use std::ffi::{c_char, c_double, c_int, c_void, CString};
use std::ptr;

pub type Handle = *mut c_void;
type Wakeup = Option<unsafe extern "C" fn(*mut c_void)>;

pub const EVENT_NONE: c_int = 0;
pub const EVENT_SHUTDOWN: c_int = 1;
pub const EVENT_COMMAND_REPLY: c_int = 5;
pub const EVENT_END_FILE: c_int = 7;
pub const EVENT_FILE_LOADED: c_int = 8;
pub const EVENT_PLAYBACK_RESTART: c_int = 21;
pub const EVENT_PROPERTY_CHANGE: c_int = 22;
pub const FORMAT_STRING: c_int = 1;
pub const FORMAT_FLAG: c_int = 3;
pub const FORMAT_INT64: c_int = 4;
pub const FORMAT_DOUBLE: c_int = 5;
pub const END_FILE_ERROR: c_int = 4;
pub const ERROR_NOTHING_TO_PLAY: c_int = -16;
pub const ERROR_UNKNOWN_FORMAT: c_int = -17;
pub const RENDER_API_TYPE: c_int = 1;
pub const RENDER_NEXT_FRAME_INFO: c_int = 11;
pub const RENDER_SW_SIZE: c_int = 17;
pub const RENDER_SW_FORMAT: c_int = 18;
pub const RENDER_SW_STRIDE: c_int = 19;
pub const RENDER_SW_POINTER: c_int = 20;
pub const RENDER_UPDATE_FRAME: u64 = 1;
pub const FRAME_INFO_PRESENT: u64 = 1;
pub const SEEK_REPLY: u64 = 1;
pub const FILTER_REPLY: u64 = 2;

#[repr(C)]
pub struct Event {
    pub id: c_int,
    pub error: c_int,
    pub userdata: u64,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct Property {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct EndFile {
    pub reason: c_int,
    pub error: c_int,
}

#[repr(C)]
pub struct Param {
    pub kind: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct FrameInfo {
    pub flags: u64,
    pub target_time: i64,
}

pub struct Api {
    pub create: unsafe extern "C" fn() -> Handle,
    pub initialize: unsafe extern "C" fn(Handle) -> c_int,
    pub set_option: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    pub command: unsafe extern "C" fn(Handle, u64, *const *const c_char) -> c_int,
    pub get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    pub observe: unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int,
    pub wait_event: unsafe extern "C" fn(Handle, c_double) -> *const Event,
    pub set_wakeup: unsafe extern "C" fn(Handle, Wakeup, *mut c_void),
    pub terminate: unsafe extern "C" fn(Handle),
    pub render_create: unsafe extern "C" fn(*mut Handle, Handle, *mut Param) -> c_int,
    pub render_update: unsafe extern "C" fn(Handle) -> u64,
    pub render_info: unsafe extern "C" fn(Handle, Param) -> c_int,
    pub render: unsafe extern "C" fn(Handle, *mut Param) -> c_int,
    pub render_callback: unsafe extern "C" fn(Handle, Wakeup, *mut c_void),
    pub render_free: unsafe extern "C" fn(Handle),
}

impl Api {
    pub fn load() -> Result<Self, String> {
        let library = Library::open("libmpv.so.2")?;
        Ok(symbols!(library, Api {
            create: unsafe extern "C" fn() -> Handle = "mpv_create",
            initialize: unsafe extern "C" fn(Handle) -> c_int = "mpv_initialize",
            set_option: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int = "mpv_set_option_string",
            command: unsafe extern "C" fn(Handle, u64, *const *const c_char) -> c_int = "mpv_command_async",
            get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int = "mpv_get_property",
            observe: unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int = "mpv_observe_property",
            wait_event: unsafe extern "C" fn(Handle, c_double) -> *const Event = "mpv_wait_event",
            set_wakeup: unsafe extern "C" fn(Handle, Wakeup, *mut c_void) = "mpv_set_wakeup_callback",
            terminate: unsafe extern "C" fn(Handle) = "mpv_terminate_destroy",
            render_create: unsafe extern "C" fn(*mut Handle, Handle, *mut Param) -> c_int = "mpv_render_context_create",
            render_update: unsafe extern "C" fn(Handle) -> u64 = "mpv_render_context_update",
            render_info: unsafe extern "C" fn(Handle, Param) -> c_int = "mpv_render_context_get_info",
            render: unsafe extern "C" fn(Handle, *mut Param) -> c_int = "mpv_render_context_render",
            render_callback: unsafe extern "C" fn(Handle, Wakeup, *mut c_void) = "mpv_render_context_set_update_callback",
            render_free: unsafe extern "C" fn(Handle) = "mpv_render_context_free",
        }))
    }
}

pub fn check(status: c_int, what: &str) -> Result<(), String> {
    if status < 0 {
        Err(format!("{what}: mpv error {status}"))
    } else {
        Ok(())
    }
}

pub fn text(value: &str) -> CString {
    CString::new(value).unwrap_or_default()
}

/// Async commands keep the render thread from waiting on the core while the core waits on a frame.
pub fn command(api: &Api, handle: Handle, words: &[&str]) -> Result<(), String> {
    let owned: Vec<CString> = words.iter().map(|word| text(word)).collect();
    let mut list: Vec<*const c_char> = owned.iter().map(|word| word.as_ptr()).collect();
    list.push(ptr::null());
    let reply = match words {
        ["seek", ..] => SEEK_REPLY,
        ["set", "vf", _] => FILTER_REPLY,
        _ => 0,
    };
    // SAFETY: mpv_command_async copies the null-terminated list of live strings before returning.
    check(
        unsafe { (api.command)(handle, reply, list.as_ptr()) },
        words[0],
    )
}

pub fn set(api: &Api, handle: Handle, name: &str, value: &str) -> Result<(), String> {
    command(api, handle, &["set", name, value])
}

pub fn get_double(api: &Api, handle: Handle, name: &str) -> Option<f64> {
    let name = text(name);
    let mut value = 0_f64;
    // SAFETY: a double property read into a double.
    let status = unsafe {
        (api.get_property)(
            handle,
            name.as_ptr(),
            FORMAT_DOUBLE,
            (&mut value as *mut f64).cast(),
        )
    };
    (status >= 0 && value.is_finite()).then_some(value)
}

/// A restart can be queued from an earlier seek; only current completed playback certifies its endpoint.
pub fn completed_seek(api: &Api, handle: Handle, target: i64) -> Option<(bool, bool, bool)> {
    if get_flag(api, handle, "seeking")? {
        return None;
    }
    let paused = get_flag(api, handle, "pause")?;
    let eof = get_flag(api, handle, "eof-reached")?;
    let duration = get_double(api, handle, "duration")? * 1000.;
    // A paused video can land on its final frame before duration without raising decoder EOF.
    let at_end = duration > 0. && target as f64 >= duration.round();
    Some((paused, eof, at_end))
}

pub fn get_flag(api: &Api, handle: Handle, name: &str) -> Option<bool> {
    let name = text(name);
    let mut value: c_int = 0;
    // SAFETY: a flag property is an int read into a live int of the same size.
    let status = unsafe {
        (api.get_property)(
            handle,
            name.as_ptr(),
            FORMAT_FLAG,
            (&mut value as *mut c_int).cast(),
        )
    };
    (status >= 0).then_some(value != 0)
}
