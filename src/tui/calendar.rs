use crate::jsondoc::Json;
use std::ffi::{c_int, c_long};

// The full Linux libc tm layout includes the offset and zone after the nine calendar fields.
#[repr(C)]
struct Broken {
    seconds: c_int,
    minutes: c_int,
    hours: c_int,
    day: c_int,
    month: c_int,
    year: c_int,
    weekday: c_int,
    yearday: c_int,
    daylight: c_int,
    offset: c_long,
    zone: *const u8,
}
extern "C" {
    fn localtime_r(time: *const i64, result: *mut Broken) -> *mut Broken;
}

pub fn modified(seconds: i64) -> String {
    if seconds <= 0 { return String::new(); }
    let mut local: Broken = unsafe { std::mem::zeroed() };
    if unsafe { localtime_r(&seconds, &mut local) }.is_null() { return String::new(); }
    let Some(year) = local.year.checked_add(1900) else { return String::new(); };
    stamp(year, local.month + 1, local.day, local.hours, local.minutes)
}

fn stamp(year: i32, month: i32, day: i32, hour: i32, minute: i32) -> String {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month)
        || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) { return String::new(); }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month { 2 => 28 + i32::from(leap), 4 | 6 | 9 | 11 => 30, _ => 31 };
    if !(1..=days).contains(&day) { return String::new(); }
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

// GIO Trash supplies local wall time without an offset; converting it as UTC would shift Deleted.
pub fn value(value: &Json) -> String {
    if let Json::Num(number) = value { return number.parse().map(modified).unwrap_or_default(); }
    let Some(text) = value.as_str() else { return String::new(); };
    let bytes = text.as_bytes();
    if bytes.len() != 19 || !bytes.iter().enumerate().all(|(i, b)| match i {
        4 | 7 => *b == b'-', 10 => *b == b'T', 13 | 16 => *b == b':', _ => b.is_ascii_digit(),
    }) { return String::new(); }
    let number = |start, end| text[start..end].parse::<i32>().unwrap_or(-1);
    if number(17, 19) > 59 { return String::new(); }
    stamp(number(0, 4), number(5, 7), number(8, 10), number(11, 13), number(14, 16))
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
