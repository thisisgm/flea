use super::Locale;
use crate::backend::trashdate::{local_time, WallClock};
use std::ffi::{c_char, c_int, c_void, CStr, CString};

extern "C" {
    fn newlocale(mask: c_int, name: *const c_char, base: *mut c_void) -> *mut c_void;
    fn freelocale(locale: *mut c_void);
    fn nl_langinfo_l(item: c_int, locale: *mut c_void) -> *mut c_char;
    fn strftime_l(out: *mut c_char, size: usize, format: *const c_char, tm: *const WallClock, locale: *mut c_void) -> usize;
}

struct Handle(*mut c_void);

impl Handle {
    fn new(name: &str, mask: c_int) -> Option<Self> {
        let name = CString::new(name).ok()?;
        let handle = unsafe { newlocale(mask, name.as_ptr(), std::ptr::null_mut()) };
        if handle.is_null() { None } else { Some(Self(handle)) }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { freelocale(self.0) };
    }
}

pub(super) fn set_grouping(locale: &mut Locale) {
    if let Some(handle) = Handle::new(&locale.glibc_locale, 1 << 1) {
        // _NL_ITEM(LC_NUMERIC, 1) and _NL_ITEM(LC_NUMERIC, 2) are separator and grouping.
        unsafe {
            locale.separator = CStr::from_ptr(nl_langinfo_l(0x10001, handle.0)).to_string_lossy().into_owned();
            locale.grouping = CStr::from_ptr(nl_langinfo_l(0x10002, handle.0)).to_bytes().to_vec();
        }
        return;
    }
    // Omarchy normally generates the locale first; keep useful grouping on a bare development host too.
    let language = locale.format_locale.split('-').next().unwrap_or("en");
    locale.separator = match language {
        "fr" => "\u{202f}",
        "ru" | "uk" | "pl" | "cs" | "sk" | "sv" | "fi" | "no" | "et" | "lv" | "lt" | "hu" => "\u{a0}",
        "fa" => "٬",
        _ if locale.decimal == "," => ".",
        _ => ",",
    }.into();
    locale.grouping = if ["hi", "bn"].contains(&language) { vec![3, 2] } else { vec![3] };
}

impl Locale {
    pub fn localize_number(&self, value: &str) -> String {
        let number = value.strip_prefix('-').unwrap_or(value);
        let mut parts = number.split('.');
        let integer = parts.next().unwrap_or("");
        let fraction = parts.next();
        if integer.is_empty() || !integer.bytes().all(|b| b.is_ascii_digit())
            || fraction.is_some_and(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
            || parts.next().is_some() {
            return value.into();
        }
        self.digits(&value.replace('.', &self.decimal))
    }

    pub fn format_number(&self, value: f64, decimals: usize) -> String {
        self.localize_number(&format!("{:.*}", decimals.min(20), value))
    }

    pub fn format_count(&self, count: i64) -> String {
        let text = count.unsigned_abs().to_string();
        let mut end = text.len();
        let mut groups = Vec::new();
        let mut index = 0;
        while end > 0 {
            let size = self.grouping.get(index).or_else(|| self.grouping.last()).copied().unwrap_or(127);
            let start = if size == 0 || size >= 127 { 0 } else { end.saturating_sub(size as usize) };
            groups.push(&text[start..end]);
            end = start;
            if index + 1 < self.grouping.len() { index += 1; }
        }
        groups.reverse();
        let sign = if count < 0 { "-" } else { "" };
        self.digits(&format!("{sign}{}", groups.join(&self.separator)))
    }

    pub fn format_size(&self, bytes: u64) -> String {
        const UNITS: [&str; 7] = ["B", "kB", "MB", "GB", "TB", "PB", "EB"];
        let mut value = bytes as f64;
        let mut unit = 0;
        while value >= 1000.0 && unit < UNITS.len() - 1 {
            value /= 1000.0;
            unit += 1;
        }
        let number = if unit == 0 { self.localize_number(&bytes.to_string()) } else { self.format_number(value, 1) };
        format!("{number} {}", UNITS[unit])
    }

    // The fixed date form is shared by all languages; only digits and the locale handle vary.
    pub fn format_date(&self, timestamp: i64) -> Option<String> {
        let tm = local_time(timestamp)?;
        let locale = Handle::new(&self.glibc_locale, 1 << 2).or_else(|| Handle::new("C", 1 << 2))?;
        let mut out = [0u8; 128];
        let length = unsafe {
            strftime_l(out.as_mut_ptr().cast(), out.len(), c"%Y-%m-%d %H:%M".as_ptr(), &tm, locale.0)
        };
        if length == 0 { return None; }
        Some(self.digits(std::str::from_utf8(&out[..length]).ok()?))
    }

    fn digits(&self, text: &str) -> String {
        text.chars().map(|c| {
            if c.is_ascii_digit() {
                char::from_u32(self.zero_digit as u32 + c as u32 - '0' as u32).unwrap_or(c)
            } else { c }
        }).collect()
    }
}
