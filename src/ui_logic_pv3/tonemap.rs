//! PQ clips tone-mapped on libmpv's threaded filter chain, so its software renderer no longer converts every 4K frame on one thread.
use super::picture::decode_size;
use std::time::Duration;

/// How long frames may stop under the filter, while play should be moving, before the filter is given up.
pub const STALL: Duration = Duration::from_secs(2);

/// What the decoder reports about the clip, before any filter runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Source {
    pub pq: bool,
    pub width: u32,
    pub height: u32,
    /// libmpv 0.41's software renderer aborts on a rotated clip either way, so the filter leaves it alone.
    pub rotated: bool,
}

/// Only PQ: the renderer already keeps up with HLG at 4K, where the filter would cost about 40% more CPU.
pub fn pq(gamma: &str) -> bool {
    gamma == "pq"
}

/// mpv's vf value for a decoded size, matching the renderer's own BT.709 picture; empty takes the filter off.
pub fn filter(size: Option<(u32, u32)>) -> String {
    // setsar=1 keeps mpv's reported size the size asked for, so the clip size never feeds back into a new filter.
    match size {
        Some((w, h)) => format!(
            "lavfi=[zscale=w={w}:h={h}:f=lanczos:d=random:t=bt709:p=bt709:m=gbr:r=pc,format=gbrp,setsar=1]"
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
    /// The size the filter should have for this box: none for a clip that is not PQ, rotated or unsized, or once given up.
    pub fn wanted(&self, box_w: u32, box_h: u32) -> Option<(u32, u32)> {
        let source = self.source;
        if self.failed || !source.pq || source.rotated || source.width == 0 || source.height == 0 {
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
