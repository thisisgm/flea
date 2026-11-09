//! The media strip's geometry and its two press machines, the seek scrub and the volume level.
use super::Sound;

/// Media040 draws a 60 px volume track at bodySmall 13, in resolved pixels.
pub fn volume_width(body_small: f32) -> f32 {
    (60. * body_small / 13.).round()
}

/// A square handle sized off the caption token.
pub fn handle(caption: f32) -> f32 {
    (caption * 0.5).round()
}

/// A part's left edge and width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub x: f32,
    pub w: f32,
}

impl Span {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn contains(&self, x: f32) -> bool {
        x >= self.x && x < self.right()
    }
}

/// The token sizes a strip lays out from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub gap: f32,
    pub slot: f32,
    pub body_small: f32,
}

/// Where each part of a strip sits, left to right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub play: Span,
    pub seek: Span,
    pub clock: Span,
    pub mute: Span,
    pub volume: Option<Span>,
}

/// MediaStrip's anchors: play and seek from the left, the volume track (unframed only), mute and clock from the right.
pub fn layout(width: f32, framed: bool, clock_width: f32, metrics: Metrics) -> Layout {
    let Metrics { gap, slot, .. } = metrics;
    let play = Span { x: gap, w: slot };
    let volume = (!framed).then(|| {
        let w = volume_width(metrics.body_small);
        Span {
            x: width - gap - w,
            w,
        }
    });
    let mute_right = volume.map_or(width - gap, |track| track.x - gap);
    let mute = Span {
        x: mute_right - slot,
        w: slot,
    };
    let clock = Span {
        x: mute.x - gap - clock_width,
        w: clock_width,
    };
    let seek_x = play.right() + gap;
    let seek = Span {
        x: seek_x,
        w: (clock.x - gap - seek_x).max(0.),
    };
    Layout {
        play,
        seek,
        clock,
        mute,
        volume,
    }
}

/// The played part of a clip, 0 with no duration yet.
pub fn fraction(position: i64, duration: i64) -> f32 {
    if duration > 0 {
        (position as f64 / duration as f64).clamp(0., 1.) as f32
    } else {
        0.
    }
}

/// A point on a track as a fraction of it.
pub fn fraction_at(x: f32, width: f32) -> f32 {
    (x / width.max(1.)).clamp(0., 1.)
}

/// A point on the volume track as a level.
pub fn level_at(x: f32, width: f32) -> u8 {
    (100. * fraction_at(x, width)).round() as u8
}

/// The handle's left edge on a track of this width at this fraction.
pub fn handle_x(width: f32, fraction: f32, handle: f32) -> f32 {
    (width * fraction).round() - handle / 2.
}

/// A drag on the seek track: drawn from the pointer, sought once on release.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scrub {
    pub active: bool,
    pub fraction: f32,
}

impl Scrub {
    pub fn press(&mut self, x: f32, width: f32) {
        self.active = true;
        self.fraction = fraction_at(x, width);
    }

    pub fn moved(&mut self, x: f32, width: f32) {
        if self.active {
            self.fraction = fraction_at(x, width);
        }
    }

    /// The absolute seek a release asks for, none with no duration to scale it.
    pub fn release(&mut self, duration: i64) -> Option<i64> {
        self.active = false;
        (duration > 0).then(|| (duration as f64 * f64::from(self.fraction)) as i64)
    }

    /// What the track draws: the pointer while dragging, the player otherwise.
    pub fn shown(&self, played: f32) -> f32 {
        if self.active {
            self.fraction
        } else {
            played
        }
    }
}

/// A wheel click over the seek track: one step, the host clamps it.
pub fn wheel_seek(position: i64, duration: i64, up: bool) -> Option<i64> {
    let step = super::player::WHEEL_STEP_MS;
    (duration > 0).then(|| position + if up { step } else { -step })
}

/// The volume track's press: the level follows it live and is kept on release or a stolen grab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Level {
    pub pressing: bool,
}

impl Level {
    pub fn press(&mut self, sound: &mut Sound, x: f32, width: f32) {
        self.pressing = true;
        sound.set_volume(level_at(x, width));
    }

    pub fn moved(&mut self, sound: &mut Sound, x: f32, width: f32, button_down: bool) {
        if button_down && self.pressing {
            sound.set_volume(level_at(x, width));
        }
    }

    /// A release or a cancel keeps the level heard.
    pub fn release(&mut self, sound: &mut Sound) -> Option<u8> {
        self.pressing = false;
        sound.keep_volume()
    }

    /// The strip going away under a held press lets the level go.
    pub fn destroyed(&mut self, sound: &mut Sound) {
        if self.pressing {
            self.pressing = false;
            sound.drop_press();
        }
    }
}
