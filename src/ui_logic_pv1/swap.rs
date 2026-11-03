// The preview swap's decisions: the old picture holds until the new preview is whole, or the cap shows loading.
use super::kinds::State;

// Both swaps' cap: with four 120 Hz frames for input, the timer and the draw, the mark shows by 100 ms.
pub const CAP_MS: i64 = 66;
// The hold-off of a loading state nothing keeps MIN_SHOW_MS.
pub const HOLD_MS: i64 = 150;
// A drawn loading state stands this long before content replaces it; under 100 ms the two read as one blink.
pub const MIN_SHOW_MS: i64 = 100;
// A PDF opens its document only this long after its path settles (issue 117).
pub const DOCUMENT_SETTLE_MS: i64 = 120;
// A capture draws on the next frame; one not drawn in this long means the window is not drawing.
pub const CAPTURE_MS: i64 = 100;

// How long the new preview may build under the held picture; a PDF's document settle runs under it.
pub fn cap_ms(_is_pdf: bool) -> i64 { CAP_MS }

// How much longer a loading state drawn at since stands before it may go; 0 when none is drawn.
pub fn show_left(since: i64, now: i64) -> i64 {
    if since > 0 {
        (since + MIN_SHOW_MS - now).max(0)
    } else {
        0
    }
}

// When a held preview lets go: None while the new one is not whole, else the ms a loading state is still owed.
pub fn release_in(ready: bool, started: bool, shown_at: i64, now: i64) -> Option<i64> {
    if !ready || !started {
        return None;
    }
    Some(show_left(shown_at, now))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallBack {
    pub shown_at: i64,
    pub left: i64,
    pub keeps: bool,
}

// Past the cap the loading state stands from shown_at or now; fall_back(true, 0, 1066) is (1066, 100, true).
pub fn fall_back(held: bool, shown_at: i64, now: i64) -> FallBack {
    let at = if shown_at > 0 { shown_at } else { now };
    let left = show_left(at, now);
    FallBack { shown_at: at, left, keeps: held && left > 0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Hold,
    Join,
    Burst,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MoveState {
    pub capturing: bool,
    pub holding: bool,
    pub key: Option<String>,
}

// A move starts a hold, joins the one starting for this key, or, where a burst may end it, gives up the picture.
pub fn on_move(state: &MoveState, key: &str, burst_ends: bool) -> Move {
    if !state.capturing && !state.holding {
        return Move::Hold;
    }
    if state.key.as_deref() == Some(key) || !burst_ends {
        return Move::Join;
    }
    Move::Burst
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    // A whole preview drawn live.
    Whole,
    Held,
    Loading,
    // The defect a hold exists to prevent: a half-built preview drawn live.
    Mid,
}

pub fn frame_kind(held: bool, ready: bool, fell_back: bool, showing: bool) -> Frame {
    if showing {
        Frame::Loading
    } else if held {
        Frame::Held
    } else if ready {
        Frame::Whole
    } else if fell_back {
        Frame::Loading
    } else {
        Frame::Mid
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStatus {
    Null,
    Ready,
    Loading,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnProbe {
    pub state: State,
    pub thumb: bool,
    pub frame: ImageStatus,
    pub no_thumb_coming: bool,
    pub pdf_drawn: bool,
    pub pdf_failed: bool,
    pub lines_loading: bool,
    pub meta: bool,
}

impl ColumnProbe {
    pub fn new(state: State) -> Self {
        Self {
            state, thumb: false, frame: ImageStatus::Null, no_thumb_coming: false, pdf_drawn: false, pdf_failed: false,
            lines_loading: false, meta: false,
        }
    }
}

// The preview column's cursor row is whole when the work its kind draws has landed.
pub fn column_ready(p: &ColumnProbe) -> bool {
    let drawn = matches!(p.frame, ImageStatus::Ready | ImageStatus::Error);
    match p.state {
        State::Loading => false,
        State::Image => (p.thumb || p.no_thumb_coming) && drawn,
        State::Video => if p.thumb { drawn } else { p.no_thumb_coming },
        State::Pdf => p.pdf_drawn || p.pdf_failed,
        State::Text | State::Code => !p.lines_loading,
        State::Archive => p.meta,
        _ => true,
    }
}

// Quick Look is ready when nothing loads, a PDF shows a page or is refused, or an interim or Markdown head shows.
pub fn look_ready(loading: bool, is_pdf: bool, pdf_shown: bool, pdf_failed: bool, interim_shown: bool, head_shown: bool) -> bool {
    if interim_shown || head_shown {
        return true;
    }
    if loading {
        return false;
    }
    !is_pdf || pdf_shown || pdf_failed
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

// JavaScript's Math.round: halves round toward positive infinity.
pub fn js_round(value: f64) -> f64 { (value + 0.5).floor() }

// interim_rect(754, 471, 6000, 4000, 1) is 706.5x471 at 24,0: upright aspect-fit, never enlarged.
pub fn interim_rect(surface_w: f64, surface_h: f64, image_w: f64, image_h: f64, orient: u32) -> Option<Rect> {
    if image_w.is_nan() || image_h.is_nan() || image_w <= 0.0 || image_h <= 0.0 {
        return None;
    }
    let (ow, oh) = if orient >= 5 { (image_h, image_w) } else { (image_w, image_h) };
    let s = 1f64.min(surface_w / ow).min(surface_h / oh);
    let (w, h) = (ow * s, oh * s);
    Some(Rect { x: js_round((surface_w - w) / 2.0), y: js_round((surface_h - h) / 2.0), w, h })
}
