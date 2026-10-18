// The local wall clock a .trashinfo DeletionDate is stamped from, split out of hometrash.rs.
#[repr(C)]
struct WallClock {
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

// Properties uses the same local wall clock as the rest of the desktop, with minute precision.
pub(super) fn modified_local(seconds: i64) -> Result<String, String> {
    let mut out = WallClock { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, gmtoff: 0, zone: std::ptr::null() };
    if unsafe { localtime_r(&seconds, &mut out) }.is_null() {
        return Err("Could not format the selected items' modification dates.".into());
    }
    Ok(format!("{:04}-{:02}-{:02} {:02}:{:02}", out.year + 1900, out.mon + 1, out.mday, out.hour, out.min))
}

// Stamps a .trashinfo DeletionDate in local time, the way the desktop's own trash names it.
pub(crate) fn deletion_date_local() -> String {
    let now = unsafe { time(std::ptr::null_mut()) };
    let mut out = WallClock { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, gmtoff: 0, zone: std::ptr::null() };
    if unsafe { localtime_r(&now, &mut out) }.is_null() {
        return "1970-01-01T00:00:00".to_string();
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", out.year + 1900, out.mon + 1, out.mday, out.hour, out.min, out.sec)
}
