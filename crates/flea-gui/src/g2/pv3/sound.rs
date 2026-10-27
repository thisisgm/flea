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
    /// Seeds the level from the state file and names where a kept one is written.
    pub fn install(stored: u8, keep: Option<KeepVolume>, cx: &mut App) {
        cx.set_global(Self {
            sound: Sound::new(stored),
            keep,
        });
    }

    /// The focused window supplies the writer while preserving the shared session sound.
    pub fn set_keep(keep: KeepVolume, cx: &mut App) {
        cx.update_global::<Self, _>(|this, _| this.keep = Some(keep));
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
