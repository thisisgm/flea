//! Media preview logic shared by every frontend: the level and mute, the clock and facts, and the player's projections.
pub mod frame;
pub mod picture;
pub mod player;
pub mod session;
pub mod strip;
pub mod tonemap;
pub mod transport;

#[cfg(test)]
#[path = "ui_logic_media_preview/frame_tests.rs"]
mod frame_tests;
#[cfg(test)]
#[path = "ui_logic_media_preview/player_tests.rs"]
mod player_tests;
#[cfg(test)]
#[path = "ui_logic_media_preview/sound_tests.rs"]
mod sound_tests;
#[cfg(test)]
#[path = "ui_logic_media_preview/strip_tests.rs"]
mod strip_tests;
#[cfg(test)]
#[path = "ui_logic_media_preview/tonemap_tests.rs"]
mod tonemap_tests;
#[cfg(test)]
mod transport_seek_tests;
#[cfg(test)]
#[path = "ui_logic_media_preview/transport_tests.rs"]
mod transport_tests;

/// The level a fresh profile plays at, the full-scale output a player had before the level existed.
pub const FULL_LEVEL: u8 = 100;

/// QtAudio.convertVolume(level / 100, Logarithmic, Linear) as qtaudio.cpp computes it.
pub fn gain(level: u8) -> f64 {
    if level > 99 {
        1.
    } else {
        -(1. - f64::from(level) / 100.).ln() / 100_f64.ln()
    }
}

/// MediaMute rule 3: one session flag both media surfaces read, and the level held under a press or stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sound {
    pub muted: bool,
    /// The level under a press, none with no press down.
    pub held: Option<u8>,
    /// The level the state file holds, `preview.volume`.
    pub stored: u8,
}

impl Sound {
    pub fn new(stored: u8) -> Self {
        Self {
            muted: false,
            held: None,
            stored: stored.min(FULL_LEVEL),
        }
    }

    /// The level every player plays at, mute or not.
    pub fn volume(&self) -> u8 {
        self.held.unwrap_or(self.stored)
    }

    /// What the player takes, mute leaving it alone.
    pub fn gain(&self) -> f64 {
        gain(self.volume())
    }

    pub fn toggle(&mut self) {
        self.muted = !self.muted;
    }

    /// A press or drag sets the level live and writes nothing.
    pub fn set_volume(&mut self, level: u8) {
        self.held = Some(level.min(FULL_LEVEL));
    }

    /// The release keeps the level once; the level to write, none when the file already holds it.
    pub fn keep_volume(&mut self) -> Option<u8> {
        let level = self.volume();
        self.held = None;
        if level == self.stored {
            return None;
        }
        self.stored = level;
        Some(level)
    }

    /// A strip destroyed under a held press drops the dragged level rather than keeping it.
    pub fn drop_press(&mut self) {
        self.held = None;
    }

    /// A screen reader's step: the level moves by `delta` and is kept as a release keeps it.
    pub fn nudge(&mut self, delta: i32) -> Option<u8> {
        let level = (i32::from(self.volume()) + delta).clamp(0, i32::from(FULL_LEVEL));
        self.set_volume(level as u8);
        self.keep_volume()
    }
}

/// The one patch a kept level writes to ui.json.
pub fn volume_patch(level: u8) -> String {
    format!("{{\"preview\":{{\"volume\":{level}}}}}")
}

/// Format.duration: "3:05", or "1:03:05" once an hour is on the clock.
pub fn duration(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (hours, minutes, seconds) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Format.sampleRate's number in kilohertz: whole without a decimal, else one; none for a file with no rate.
pub fn sample_rate_khz(hz: i64) -> Option<String> {
    if hz <= 0 {
        return None;
    }
    if hz % 1000 == 0 {
        Some((hz / 1000).to_string())
    } else {
        Some(format!("{:.1}", hz as f64 / 1000.))
    }
}

/// The name a path ends in.
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// MediaPdf rule 6, in order: the kind or the suffix, the duration, the rate, the size; empty parts left out.
pub fn facts(path: &str, kind_name: &str, duration_ms: i64, rate: &str, size: &str) -> Vec<String> {
    let name = file_name(path);
    let suffix = match name.rfind('.') {
        Some(dot) if dot > 0 => name[dot + 1..].to_lowercase(),
        _ => String::new(),
    };
    let kind = if kind_name.is_empty() {
        suffix
    } else {
        kind_name.to_string()
    };
    let clock = if duration_ms > 0 {
        duration(duration_ms)
    } else {
        String::new()
    };
    [kind, clock, rate.to_string(), size.to_string()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect()
}

/// Format.fileUri: encodeURI, then `#` and `?` escaped too.
pub fn file_uri(path: &str) -> String {
    const KEPT: &[u8] = b";,/:@&=+$-_.!~*'()";
    let mut out = String::with_capacity(path.len() + 7);
    out.push_str("file://");
    for &byte in
        std::os::unix::ffi::OsStrExt::as_bytes(crate::bytename::text_path(path).as_os_str())
    {
        if byte.is_ascii_alphanumeric() || KEPT.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
#[path = "ui_logic_media_preview/preview_quality_tests.rs"]
mod preview_quality_tests;
