//! The one sound both media surfaces share: mute for the session, the level kept as preview.volume.
use flea::ui_logic_pv3::{Sound, FULL_LEVEL};
use gpui::{App, BorrowAppContext, Global};
use std::rc::Rc;

/// Where a kept level goes; G1's persisted view state, a stand-in that keeps nothing until it lands.
pub type KeepVolume = Rc<dyn Fn(u8, &mut App)>;

pub struct MediaSound {
    pub sound: Sound,
    keep: Option<KeepVolume>,
}

impl Global for MediaSound {}

impl Default for MediaSound {
    fn default() -> Self {
        Self {
            sound: Sound::new(FULL_LEVEL),
            keep: None,
        }
    }
}

impl MediaSound {
    /// ViewState.mediaVolume: the state file's preview.volume, full scale when it has none.
    pub fn stored(data: &flea::jsondoc::Json) -> u8 {
        data.get("preview")
            .and_then(|preview| preview.get("volume"))
            .and_then(flea::jsondoc::Json::as_f64)
            .map_or(FULL_LEVEL, |level| {
                level.clamp(0., f64::from(FULL_LEVEL)).round() as u8
            })
    }

    /// Seeds the level from the state file and names where a kept one is written.
    pub fn install(stored: u8, keep: Option<KeepVolume>, cx: &mut App) {
        cx.set_global(Self {
            sound: Sound::new(stored),
            keep,
        });
    }

    pub fn get(cx: &App) -> Sound {
        cx.try_global::<Self>()
            .map_or(Sound::new(FULL_LEVEL), |this| this.sound)
    }

    /// Changes the shared sound; every player observing the global follows.
    pub fn update<R>(cx: &mut App, change: impl FnOnce(&mut Sound) -> R) -> R {
        cx.default_global::<Self>();
        cx.update_global::<Self, R>(|this, _| change(&mut this.sound))
    }

    /// Writes a level a release or a nudge kept, once.
    pub fn kept(level: Option<u8>, cx: &mut App) {
        let keep = cx.try_global::<Self>().and_then(|this| this.keep.clone());
        if let (Some(level), Some(keep)) = (level, keep) {
            keep(level, cx);
        }
    }
}
