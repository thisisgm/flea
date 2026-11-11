// The local wall clock a .trashinfo DeletionDate is stamped from, split out of hometrash.rs.
#[repr(C)]
pub struct WallClock {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: *const u8,
}

extern "C" {
    fn time(t: *mut i64) -> i64;
    fn localtime_r(timep: *const i64, result: *mut WallClock) -> *mut WallClock;
}

// Shared by localized dates so both callers use the same libc declaration and struct tm layout.
pub fn local_time(timestamp: i64) -> Option<WallClock> {
    let mut out = std::mem::MaybeUninit::<WallClock>::uninit();
    // SAFETY: both pointers are valid, and libc initializes the whole tm on a non-null return.
    if unsafe { localtime_r(&timestamp, out.as_mut_ptr()) }.is_null() {
        None
    } else {
        Some(unsafe { out.assume_init() })
    }
}

// Properties uses the same local wall clock as the rest of the desktop, with minute precision.
pub fn modified_local(seconds: i64) -> Result<String, String> {
    let out = local_time(seconds)
        .ok_or_else(|| "Could not format the selected items' modification dates.".to_string())?;
    Ok(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        out.year + 1900,
        out.mon + 1,
        out.mday,
        out.hour,
        out.min
    ))
}

// Stamps a .trashinfo DeletionDate in local time, the way the desktop's own trash names it.
pub fn deletion_date_local() -> String {
    let now = unsafe { time(std::ptr::null_mut()) };
    deletion_date_at(now)
}

// Keep the formatter independent of the clock so timezone and second boundaries have fixed oracles.
pub(crate) fn deletion_date_at(timestamp: i64) -> String {
    let Some(out) = local_time(timestamp) else {
        return "1970-01-01T00:00:00".to_string();
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        out.year + 1900,
        out.mon + 1,
        out.mday,
        out.hour,
        out.min,
        out.sec
    )
}
