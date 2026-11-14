//! HDR clips tone-mapped on libmpv's threaded filter chain: its software renderer converts PQ and HLG to BT.709 on one thread with no tone mapping, so everything above SDR white clips.
use super::picture::decode_size;
use std::time::Duration;

/// How long frames may stop under the filter, while play should be moving, before the filter is given up.
pub const STALL: Duration = Duration::from_secs(2);

/// BT.2408's HDR reference white, in nits: the filter's linear 1.0, which it brings to SDR white as mpv's own renderer does.
pub const WHITE_NITS: u32 = 203;
/// The peak used until the decoder reports one: HLG's nominal peak and the usual PQ mastering peak.
pub const DEFAULT_PEAK_NITS: u32 = 1000;
/// PQ's ceiling, the most a reported peak can mean.
pub const MAX_PEAK_NITS: u32 = 10_000;
/// Reinhard's contrast: on real PQ and HLG footage it lands closest to mpv's GPU renderer, which rolls off from the same peak.
const CONTRAST: &str = "0.4";

/// What the decoder reports about the clip, before any filter runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Source {
    pub hdr: bool,
    pub width: u32,
    pub height: u32,
    /// libmpv 0.41's software renderer aborts on a rotated clip either way, so the filter leaves it alone.
    pub rotated: bool,
    /// The signal peak in nits, kept from the first report; 0 until then.
    pub peak_nits: u32,
}

impl Source {
    /// Keeps the first peak mpv reports (in multiples of WHITE_NITS), so metadata that changes per frame never resets the filter.
    pub fn observe_peak(&mut self, sig_peak: f64) {
        if self.peak_nits == 0 && sig_peak.is_finite() && sig_peak > 0. {
            self.peak_nits = (sig_peak * f64::from(WHITE_NITS))
                .round()
                .clamp(f64::from(WHITE_NITS), f64::from(MAX_PEAK_NITS))
                as u32;
        }
    }

    /// The peak the curve rolls off from: the reported one, or DEFAULT_PEAK_NITS before any report.
    pub fn peak(&self) -> u32 {
        if self.peak_nits == 0 {
            DEFAULT_PEAK_NITS
        } else {
            self.peak_nits
        }
    }
}

/// PQ and HLG: the renderer has no tone mapping, so either one draws overexposed and oversaturated without the filter.
pub fn hdr(gamma: &str) -> bool {
    matches!(gamma, "pq" | "hlg")
}

/// mpv's vf value for a decoded size and peak: scale to linear BT.709 at the box size, roll the highlights off, then 8-bit BT.709; empty takes the filter off.
pub fn filter(size: Option<(u32, u32)>, peak_nits: u32) -> String {
    // setsar=1 keeps mpv's reported size the size asked for, so the clip size never feeds back into a new filter.
    match size {
        Some((w, h)) => {
            let peak = f64::from(peak_nits) / f64::from(WHITE_NITS);
            format!(
                "lavfi=[zscale=w={w}:h={h}:f=lanczos:t=linear:p=bt709:npl={WHITE_NITS},format=gbrpf32le,tonemap=tonemap=reinhard:param={CONTRAST}:peak={peak:.3}:desat=0,zscale=t=bt709:m=gbr:r=pc:d=random,format=gbrp,setsar=1]"
            )
        }
        None => String::new(),
    }
}

/// Each step down scales both sides of the mapped size by 3/4, so the map's work per frame falls to about 56%.
pub const STEP: (u64, u64) = (3, 4);
/// The mapped size never steps below this short side, in pixels; a smaller decode size is never stepped at all.
pub const FLOOR_SIDE: u32 = 360;
/// Load cap for a new filter size, in mapped plus decoded/DECODE_SHARE pixels a second: on minipc it starts 4K30 one step down in Quick Look and 4K60 at the floor.
pub const START_LOAD: u64 = 75_000_000;
/// A decoded pixel costs about this fraction of a mapped one on minipc.
pub const DECODE_SHARE: u64 = 8;
/// Steps past this leave every side far under the floor, and 4^MAX_STEPS still fits the u64 arithmetic.
const MAX_STEPS: u32 = 16;

/// A decode size stepped down `level` times, kept to the floor and to even sides.
pub fn stepped(size: (u32, u32), level: u32) -> (u32, u32) {
    let (w, h) = (u64::from(size.0), u64::from(size.1));
    let short = w.min(h);
    if level == 0 || short <= u64::from(FLOOR_SIDE) {
        return size;
    }
    let (mut num, mut den) = (1_u64, 1_u64);
    for _ in 0..level.min(MAX_STEPS) {
        num *= STEP.0;
        den *= STEP.1;
    }
    // Below the floor the short side is held at FLOOR_SIDE instead.
    if short * num < u64::from(FLOOR_SIDE) * den {
        (num, den) = (u64::from(FLOOR_SIDE), short);
    }
    let even = |side: u64| ((side * num / den).div_ceil(2) * 2).max(2) as u32;
    (even(w), even(h))
}

/// The fewest steps that bring a clip of `source` pixels drawn at `size`, at `rate_milli` thousandths of a frame a second, within START_LOAD, or the floor.
pub fn start_level(source: (u32, u32), size: (u32, u32), rate_milli: u32) -> u32 {
    let decoded = u64::from(source.0) * u64::from(source.1) / DECODE_SHARE;
    let mut level = 0;
    loop {
        let (w, h) = stepped(size, level);
        let load = (u64::from(w) * u64::from(h) + decoded) * u64::from(rate_milli) / 1000;
        if load <= START_LOAD || stepped(size, level + 1) == (w, h) {
            return level;
        }
        level += 1;
    }
}

/// The filter one helper run keeps: the size and peak that are on, whether it was given up, and its steps below the decode size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tonemap {
    pub source: Source,
    applied: Option<((u32, u32), u32)>,
    failed: bool,
    level: u32,
    rate_milli: u32,
    /// The decode size the start cap last ran for: a new size gets the cap again, and the level never falls.
    capped: Option<(u32, u32)>,
}

impl Tonemap {
    /// The clip's frame rate as mpv reports it, kept from the first positive report.
    pub fn set_rate(&mut self, fps: f64) {
        if self.rate_milli == 0 && fps.is_finite() && fps > 0. {
            self.rate_milli = (fps * 1000.).round().min(f64::from(u32::MAX)) as u32;
        }
    }

    pub fn rate_milli(&self) -> u32 {
        self.rate_milli
    }

    pub fn level(&self) -> u32 {
        self.level
    }

    /// One step down for this box; false when the size is at its floor, or the filter is not on.
    pub fn step(&mut self, box_w: u32, box_h: u32) -> bool {
        let Some(size) = self.wanted(box_w, box_h) else {
            return false;
        };
        if stepped(size, self.level + 1) == stepped(size, self.level) {
            return false;
        }
        self.level += 1;
        true
    }

    /// The size the filter should have for this box: none for a clip that is not HDR, rotated or unsized, or once given up.
    pub fn wanted(&self, box_w: u32, box_h: u32) -> Option<(u32, u32)> {
        let source = self.source;
        if self.failed || !source.hdr || source.rotated || source.width == 0 || source.height == 0 {
            return None;
        }
        Some(decode_size(source.width, source.height, box_w, box_h))
    }

    /// The vf to set, only when it differs from the one that is on.
    pub fn update(&mut self, box_w: u32, box_h: u32) -> Option<String> {
        let full = self.wanted(box_w, box_h);
        if let Some(full) = full.filter(|&full| self.capped != Some(full) && self.rate_milli > 0) {
            let source = (self.source.width, self.source.height);
            self.level = self.level.max(start_level(source, full, self.rate_milli));
            self.capped = Some(full);
        }
        let size = full.map(|full| stepped(full, self.level));
        let wanted = size.map(|size| (size, self.source.peak()));
        if wanted == self.applied {
            return None;
        }
        self.applied = wanted;
        Some(filter(size, self.source.peak()))
    }

    /// The filter failed, so it stays off for the rest of the run; the vf that takes it off, if it was on.
    pub fn fail(&mut self) -> Option<String> {
        self.failed = true;
        self.applied.take().map(|_| filter(None, 0))
    }

    pub fn on(&self) -> bool {
        self.applied.is_some()
    }

    /// The size the filter maps to while it is on: mpv's frames come out at it.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.applied.map(|(size, _)| size)
    }
}

/// Frames stopped under the filter for the whole bound: time to ask libmpv whether play should be moving.
pub fn stall_due(on: bool, since_progress: Duration) -> bool {
    on && since_progress >= STALL
}
