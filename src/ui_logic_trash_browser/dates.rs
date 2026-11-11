//! TrashDates.js: where an item came from, when it was deleted and whether the 30 day sweep takes it.

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

// glibc's struct tm, which mktime reads and localtime_r fills whole.
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

impl Tm {
    fn civil(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<Self> {
        Some(Self {
            sec: second as i32,
            min: minute as i32,
            hour: hour as i32,
            mday: day as i32,
            mon: month as i32 - 1,
            year: i32::try_from(year - 1900).ok()?,
            wday: 0,
            yday: 0,
            isdst: -1,
            gmtoff: 0,
            zone: std::ptr::null(),
        })
    }
}

extern "C" {
    fn mktime(tm: *mut Tm) -> i64;
    fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
}

/// The folder an item was trashed from, "~" for the home tree; None when the original names no folder.
pub fn location(original: &str, home: &str) -> Option<String> {
    let cut = original.rfind('/')?;
    let folder = if cut == 0 { "/" } else { &original[..cut] };
    if !home.is_empty() && (folder == home || folder.starts_with(&format!("{home}/"))) {
        return Some(format!("~{}", &folder[home.len()..]));
    }
    Some(folder.into())
}

fn days_in(year: i64, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's days_from_civil, so a zoned stamp needs no time zone database.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let month = month as i64;
    let of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i64 - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

fn number(text: &str, digits: usize) -> Option<u32> {
    (text.len() == digits && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok())?
}

/// Milliseconds since the epoch for an ISO stamp: local time when it names no zone, as GIO's trash::deletion-date does.
pub fn parse_ms(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once('T')?;
    let mut parts = date.split('-');
    let year = number(parts.next()?, 4)? as i64;
    let month = number(parts.next()?, 2)?;
    let day = number(parts.next()?, 2)?;
    if parts.next().is_some() || !(1..=12).contains(&month) || day == 0 || day > days_in(year, month) {
        return None;
    }
    let zone_at = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
    let (clock, zone) = rest.split_at(zone_at);
    let (clock, fraction) = clock.split_once('.').map_or((clock, None), |(c, f)| (c, Some(f)));
    let mut fields = clock.split(':');
    let hour = number(fields.next()?, 2)?;
    let minute = number(fields.next()?, 2)?;
    let second = fields.next().map_or(Some(0), |s| number(s, 2))?;
    if fields.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let millis = match fraction {
        Some(f) if !f.is_empty() && f.len() <= 9 && f.bytes().all(|b| b.is_ascii_digit()) => format!("{f:0<3}")[..3].parse::<i64>().ok()?,
        Some(_) => return None,
        None => 0,
    };
    let seconds = match zone {
        "" => {
            let mut tm = Tm::civil(year, month, day, hour, minute, second)?;
            // SAFETY: tm is a whole struct tm owned by this frame.
            let at = unsafe { mktime(&mut tm) };
            if at == -1 && !(tm.year == 69 && tm.mon == 11 && tm.mday == 31) {
                return None;
            }
            at
        }
        _ => {
            let offset = match zone {
                "Z" => 0,
                _ => {
                    let sign = if zone.starts_with('-') { -1 } else { 1 };
                    let (h, m) = zone[1..].split_once(':')?;
                    let (h, m) = (number(h, 2)? as i64, number(m, 2)? as i64);
                    if h > 23 || m > 59 {
                        return None;
                    }
                    sign * (h * 3600 + m * 60)
                }
            };
            days_from_civil(year, month, day) * 86_400 + (hour * 3600 + minute * 60 + second) as i64 - offset
        }
    };
    Some(seconds * 1000 + millis)
}

/// The deletion stamp in seconds for Locale::format_date; None draws Unknown, never a day in 1970.
pub fn deleted(text: Option<&str>) -> Option<i64> {
    parse_ms(text.filter(|t| !t.is_empty())?).map(|ms| ms.div_euclid(1000))
}

/// Whether the sweep takes an item: an unreadable or missing date is never swept, and the future is never old.
pub fn expired(text: Option<&str>, now_ms: i64, days: i64) -> bool {
    match text.filter(|t| !t.is_empty()).and_then(parse_ms) {
        Some(at) => now_ms - at >= days * DAY_MS,
        None => false,
    }
}

/// Whole days since the epoch at local midnight, which the sweep records so it runs once a day.
pub fn day_number(now_ms: i64) -> i64 {
    let seconds = now_ms.div_euclid(1000);
    let mut tm = Tm::civil(1970, 1, 1, 0, 0, 0).expect("the epoch is a civil date");
    // SAFETY: both pointers are valid for the call, and a non-null return means tm was written whole.
    let midnight = unsafe {
        if localtime_r(&seconds, &mut tm).is_null() {
            return seconds.div_euclid(86_400);
        }
        tm.sec = 0;
        tm.min = 0;
        tm.hour = 0;
        tm.isdst = -1;
        mktime(&mut tm)
    };
    (midnight * 1000).div_euclid(DAY_MS)
}
