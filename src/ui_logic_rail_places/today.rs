//! Highlight today's dates: local midnight from the wall clock, one numeric compare per row.
pub const MINUTE_MS: i64 = 60 * 1000;

// struct tm, laid out as src/backend/trashdate.rs WallClock declares it.
#[repr(C)]
struct Tm {
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
    fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    fn mktime(tm: *mut Tm) -> i64;
}

// Local midnight of the day `days` after the one now_ms falls in, built from components so a DST night still lands on midnight.
fn midnight(now_ms: i64, days: i32) -> i64 {
    let seconds = now_ms.div_euclid(1000);
    let mut tm = Tm {
        sec: 0,
        min: 0,
        hour: 0,
        mday: 0,
        mon: 0,
        year: 0,
        wday: 0,
        yday: 0,
        isdst: 0,
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    if unsafe { localtime_r(&seconds, &mut tm) }.is_null() {
        return now_ms - now_ms.rem_euclid(86_400_000) + i64::from(days) * 86_400_000;
    }
    tm.sec = 0;
    tm.min = 0;
    tm.hour = 0;
    tm.mday += days;
    tm.isdst = -1;
    (unsafe { mktime(&mut tm) }) * 1000
}

// Sample input: any ms of 2026-09-23 local answers 2026-09-23 00:00 local.
pub fn day_start(now_ms: i64) -> i64 {
    midnight(now_ms, 0)
}

// True when the stamp is on or after today's local midnight; no stamp is never recent.
pub fn is_recent(mtime_sec: Option<f64>, today_start_ms: i64) -> bool {
    mtime_sec.is_some_and(|sec| sec.is_finite() && sec * 1000.0 >= today_start_ms as f64)
}

// The one midnight timer's next arm, in ms.
pub fn ms_until_midnight(now_ms: i64) -> i64 {
    midnight(now_ms, 1) - now_ms
}
