// Quick Look's text and diff scrolling: Scroll.js's wheel arithmetic and overlay scroller over one viewport.

// One notch is the platform's lines times notchPx times the multiplier (Theme.scroll, PR 16's pair).
pub const NOTCH_PX: f64 = 24.0;
pub const MULTIPLIER: f64 = 4.0;
// Touchpad pixels move at GTK4's 2.5 gain, Finder's feel (decided 2026-10-01).
pub const TOUCH_GAIN: f64 = 2.5;
// Less than half a pixel of range is layout rounding, not content worth a bar.
pub const OVERFLOW_PX: f64 = 0.5;
// Finder's overlay scroller at base size 14: knob widths and inset in px, alphas of the foreground.
pub const KNOB_REST_PX: f64 = 6.0;
pub const KNOB_WIDE_PX: f64 = 10.0;
pub const KNOB_INSET_PX: f64 = 2.0;
pub const KNOB_ALPHA: f32 = 0.5;
pub const KNOB_HOVER_ALPHA: f32 = 0.6;
pub const KNOB_PRESSED_ALPHA: f32 = 0.7;
pub const TRACK_ALPHA: f32 = 0.05;
// Shown this long after the view stops.
pub const HOLD_MS: u64 = 1000;
// Then it fades out over this long.
pub const FADE_MS: u64 = 300;

// The bar's opacity this long after the view stopped: full through the hold, then a linear fade.
pub fn fade(elapsed_ms: f32) -> f32 {
    ((HOLD_MS as f32 + FADE_MS as f32 - elapsed_ms) / FADE_MS as f32).clamp(0.0, 1.0)
}
// Theme.hitMin: the shortest knob, so a long patch keeps one a pointer can take.
pub const HIT_MIN: f64 = 24.0;

// A wheel event: notch lines (platform lines already applied) or touchpad pixels in Qt pixelDelta units, the compositor axis value, positive toward the start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delta {
    Lines(f64),
    Pixels(f64),
}

// The travel toward the start one event asks for.
pub fn distance(delta: Delta) -> f64 {
    match delta {
        Delta::Lines(lines) => lines * NOTCH_PX * MULTIPLIER,
        Delta::Pixels(pixels) => pixels * TOUCH_GAIN,
    }
}

pub fn range(content: f64, viewport: f64) -> f64 { (content - viewport.max(0.0)).max(0.0) }

// A position kept inside its bounds; content shorter than the view pins to the origin.
pub fn bounded(at: f64, content: f64, viewport: f64) -> f64 { at.clamp(0.0, range(content, viewport)) }

// Where one wheel event leaves the view.
pub fn scrolled(at: f64, delta: Delta, content: f64, viewport: f64) -> f64 { bounded(at - distance(delta), content, viewport) }

pub fn overflows(content: f64, viewport: f64) -> bool { range(content, viewport) > OVERFLOW_PX }

// Shown while the view moves, the pointer is in the lane or a press is down, never over content that fits.
pub fn revealed(overflow: bool, moving: bool, in_lane: bool, pressed: bool) -> bool { overflow && (moving || in_lane || pressed) }

pub fn handle_length(track: f64, content: f64, viewport: f64) -> f64 {
    let (track, content, viewport) = (track.max(0.0), content.max(0.0), viewport.max(0.0));
    if track == 0.0 || content <= viewport || content == 0.0 {
        return track;
    }
    track.min(HIT_MIN.max(track * viewport / content))
}

pub fn travel(track: f64, content: f64, viewport: f64) -> f64 { (track - handle_length(track, content, viewport)).max(0.0) }

pub fn handle_offset(at: f64, content: f64, viewport: f64, track: f64) -> f64 {
    let (room, span) = (travel(track, content, viewport), range(content, viewport));
    if room == 0.0 || span == 0.0 {
        return 0.0;
    }
    room * at.clamp(0.0, span) / span
}

pub fn position_for_handle(offset: f64, content: f64, viewport: f64, track: f64) -> f64 {
    let (room, span) = (travel(track, content, viewport), range(content, viewport));
    if room == 0.0 || span == 0.0 {
        return 0.0;
    }
    span * offset.clamp(0.0, room) / room
}

// A track press puts the knob's centre under the pointer, clamped to the travel.
pub fn jump_offset(at: f64, handle: f64, room: f64) -> f64 { (at - handle / 2.0).clamp(0.0, room.max(0.0)) }

// The thumb follows Hyprland rounding: square at 0, clamped at half the thinner side.
pub fn knob_radius(corner: f64, width: f64, height: f64) -> f64 {
    if corner <= 0.0 {
        return 0.0;
    }
    corner.min(width.max(0.0).min(height.max(0.0)) / 2.0)
}

// The fixed-height rows a viewport shows at this position: first and one past the last.
pub fn rows_in_view(at: f64, margin: f64, line: f64, viewport: f64, count: usize) -> (usize, usize) {
    if line <= 0.0 || count == 0 {
        return (0, 0);
    }
    let first = ((at - margin) / line).floor().max(0.0) as usize;
    let last = ((at + viewport - margin) / line).ceil().max(0.0) as usize;
    (first.min(count), last.min(count))
}
