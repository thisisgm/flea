// Flea's text size: Omarchy's size while following, or one stop from Omarchy's own range while overriding.
use super::Text;
use crate::jsondoc::Json;
use crate::keys::presentation;

// The board's seven documented stops in px. They are not contiguous: 13, 15, 17, 18 and 19 are not stops.
pub const STOPS: [u32; 7] = [9, 10, 11, 12, 14, 16, 20];

// Omarchy's type ladder ratios from Commons/Style.qml, applied at a stop so an override matches Omarchy's own.
const BODY_SMALL_RATIO: f64 = 0.917;
const CAPTION_RATIO: f64 = 0.833;

// The stored mode that means Follow Omarchy.
const FOLLOW: &str = "system";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stored {
    Follow,
    Stop(u32),
}

// The nearest stop. A tie takes the smaller stop; NaN and Infinity both resolve to the first stop, as the oracle does.
pub fn nearest(px: f64) -> u32 {
    let mut best = STOPS[0];
    for &stop in &STOPS[1..] {
        if (f64::from(stop) - px).abs() < (f64::from(best) - px).abs() {
            best = stop;
        }
    }
    best
}

// True unless the value is a stop above zero.
pub fn following(stored: Stored) -> bool {
    !matches!(stored, Stored::Stop(px) if px > 0)
}

// The size Flea draws at: Omarchy's own while following, the nearest stop while overriding.
pub fn effective(stored: Stored, omarchy_base: u32) -> u32 {
    if following(stored) {
        omarchy_base
    } else {
        match stored {
            Stored::Stop(px) => nearest(f64::from(px)),
            Stored::Follow => omarchy_base,
        }
    }
}

// Switching to Override starts on the stop nearest the size already on screen.
pub fn pin(stored: Stored, omarchy_base: u32) -> Stored {
    Stored::Stop(nearest(f64::from(effective(stored, omarchy_base))))
}

// One stop along the list, clamped at both ends.
pub fn stepped(stored: Stored, omarchy_base: u32, direction: i32) -> Stored {
    let now = nearest(f64::from(effective(stored, omarchy_base)));
    let at = STOPS.iter().position(|&stop| stop == now).unwrap_or(0) as i64;
    let to = (at + i64::from(direction)).clamp(0, STOPS.len() as i64 - 1) as usize;
    Stored::Stop(STOPS[to])
}

// Reads the stored mode the way Number() does. Anything that is not a positive number follows Omarchy.
pub fn parse(stored: &Json) -> Stored {
    let px = mode_number(stored);
    if px > 0.0 {
        Stored::Stop(nearest(px))
    } else {
        Stored::Follow
    }
}

// The stored shape: {"mode":"system"} for Follow, {"mode":N} for a stop.
pub fn to_json(stored: &Stored) -> Json {
    let mode = match stored {
        Stored::Follow => Json::Str(FOLLOW.into()),
        Stored::Stop(px) => Json::Num(px.to_string()),
    };
    Json::Obj(vec![("mode".into(), mode)])
}

// Maps a Settings action to its step: up 1, down -1, reset 0.
pub fn direction(action: &str) -> i32 {
    match action {
        "textSizeReset" => 0,
        "textSizeUp" => 1,
        _ => -1,
    }
}

// Body text draws at the base size.
pub fn body(base: u32) -> u32 {
    base.max(1)
}

pub fn body_small(base: u32) -> u32 {
    scaled(base, BODY_SMALL_RATIO)
}

pub fn caption(base: u32) -> u32 {
    scaled(base, CAPTION_RATIO)
}

// The status bar sentence under the preset's own reset key.
pub fn announce_for(stored: Stored, omarchy_base: u32, preset: &str) -> Text {
    announce(stored, omarchy_base, &presentation::key_for(preset, "textSizeReset"))
}

// The status bar sentence for the current size and its way back; an empty reset key leaves the way back out.
pub fn announce(stored: Stored, omarchy_base: u32, reset: &str) -> Text {
    let px = Text::data(format!("{}px", effective(stored, omarchy_base)));
    if following(stored) {
        return Text::args("settings.textsize.follows_omarchy", vec![px]);
    }
    if reset.is_empty() {
        Text::args("settings.textsize.override_alone", vec![px])
    } else {
        Text::args(
            "settings.textsize.override",
            vec![px, Text::Keycap(reset.into())],
        )
    }
}

fn scaled(base: u32, ratio: f64) -> u32 {
    ((f64::from(base) * ratio).round() as u32).max(1)
}

// Number(stored.mode) for a parsed document: the last "mode" key wins, and a missing one is NaN.
fn mode_number(stored: &Json) -> f64 {
    let mode = stored
        .as_object()
        .and_then(|pairs| pairs.iter().rev().find(|(key, _)| key == "mode"))
        .map(|(_, value)| value);
    mode.map_or(f64::NAN, to_number)
}

// The JavaScript ToNumber conversion for the JSON kinds a document can hold.
fn to_number(value: &Json) -> f64 {
    match value {
        Json::Null => 0.0,
        Json::Bool(flag) => f64::from(u8::from(*flag)),
        Json::Num(text) | Json::Str(text) => js_number(text),
        Json::Arr(_) => js_number(&js_string(value)),
        Json::Obj(_) => f64::NAN,
    }
}

// The JavaScript ToString conversion, which is what an array becomes before Number() reads it.
fn js_string(value: &Json) -> String {
    match value {
        Json::Null => String::new(),
        Json::Bool(flag) => flag.to_string(),
        Json::Num(text) | Json::Str(text) => text.clone(),
        Json::Arr(items) => items.iter().map(js_string).collect::<Vec<_>>().join(","),
        Json::Obj(_) => "[object Object]".into(),
    }
}

// Number() over a string: whitespace trimmed, empty is 0, radix prefixes, Infinity, and decimals only.
fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = trimmed.strip_prefix(prefix) {
            return u64::from_str_radix(digits, radix).map_or(f64::NAN, |value| value as f64);
        }
    }
    let decimal = trimmed.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E'));
    if decimal {
        trimmed.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

#[cfg(test)]
#[path = "textsize_tests.rs"]
mod tests;
