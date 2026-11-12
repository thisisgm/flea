//! HDR clips tone-mapped on libmpv's threaded filter chain: its software renderer converts PQ and HLG to BT.709 on one thread and clips everything above SDR white.
use super::picture::decode_size;
use std::time::Duration;

/// The luminance the filter maps to SDR white: BT.2408's HDR reference white, in nits.
const WHITE_NITS: u32 = 203;
/// The peak the curve brings down to SDR white, in multiples of WHITE_NITS: 1000 nits, HLG's nominal peak and the usual PQ mastering peak.
const PEAK: &str = "4.93";
/// Where the mobius curve stops being linear, in multiples of WHITE_NITS: about 61 nits. Darker than that the picture is untouched.
/// Brighter, the roll-off begins, and reference white lands near 0.68 in linear light to leave room for the highlights.
/// It is libavfilter's default, set here so every build draws alike.
const KNEE: &str = "0.3";

/// How long frames may stop under the filter, while play should be moving, before the filter is given up.
pub const STALL: Duration = Duration::from_secs(2);

/// What the decoder reports about the clip, before any filter runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Source {
    pub hdr: bool,
    pub width: u32,
    pub height: u32,
    /// libmpv 0.41's software renderer aborts on a rotated clip either way, so the filter leaves it alone.
    pub rotated: bool,
}

/// PQ and HLG: the renderer has no tone mapping, so either one comes out overexposed and oversaturated without the filter.
pub fn hdr(gamma: &str) -> bool {
    matches!(gamma, "pq" | "hlg")
}

/// mpv's vf value for a decoded size: scale, linear light, BT.709 primaries, the highlights rolled off, then 8-bit BT.709; empty takes the filter off.
pub fn filter(size: Option<(u32, u32)>) -> String {
    // setsar=1 keeps mpv's reported size the size asked for, so the clip size never feeds back into a new filter.
    match size {
        Some((w, h)) => format!(
            "lavfi=[zscale=w={w}:h={h}:f=lanczos:t=linear:npl={WHITE_NITS},format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=mobius:param={KNEE}:peak={PEAK}:desat=0,zscale=t=bt709:m=gbr:r=pc:d=random,format=gbrp,setsar=1]"
        ),
        None => String::new(),
    }
}

/// The filter one helper run keeps: the size that is on, and whether it was given up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tonemap {
    pub source: Source,
    applied: Option<(u32, u32)>,
    failed: bool,
}

impl Tonemap {
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
        let wanted = self.wanted(box_w, box_h);
        if wanted == self.applied {
            return None;
        }
        self.applied = wanted;
        Some(filter(wanted))
    }

    /// The filter failed, so it stays off for the rest of the run; the vf that takes it off, if it was on.
    pub fn fail(&mut self) -> Option<String> {
        self.failed = true;
        self.applied.take().map(|_| filter(None))
    }

    pub fn on(&self) -> bool {
        self.applied.is_some()
    }
}

/// Frames stopped under the filter for the whole bound: time to ask libmpv whether play should be moving.
pub fn stall_due(on: bool, since_progress: Duration) -> bool {
    on && since_progress >= STALL
}
