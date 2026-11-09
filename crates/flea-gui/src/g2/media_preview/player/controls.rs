//! What a media preview's controls do: the strip, the media keys and the MPRIS helper's commands.
use super::MediaPlayer;
use crate::g2::media_preview::sound::MediaSound;
use crate::g2::media_preview::strip::{StripInput, StripView};
use flea::ui_logic_media_preview::frame::Command;
use flea::ui_logic_media_preview::player::{clamp_seek, MediaKey, Playback};
use flea::ui_logic_media_preview::session::Remote;
use flea::ui_logic_media_preview::strip::wheel_seek;
use gpui::{App, Context, Window};

impl MediaPlayer {
    /// One command from the MPRIS helper.
    pub fn remote(&mut self, remote: Remote, cx: &mut Context<Self>) {
        match remote {
            Remote::Play => self.send(Command::Play),
            Remote::Pause => self.send(Command::Pause),
            Remote::PlayPause => self.toggle(cx),
            Remote::Stop => self.send(Command::Stop),
            Remote::Seek(delta) => self.seek_to(self.position + delta),
            Remote::SetPosition(ms) => self.seek_to(ms),
        }
    }

    pub fn toggle(&mut self, _: &mut Context<Self>) {
        let command = if self.state.playback == Playback::Playing {
            Command::Pause
        } else {
            Command::Play
        };
        self.send(command);
    }

    pub fn seek_to(&mut self, ms: i64) {
        let target = clamp_seek(ms, self.duration);
        self.position = target;
        self.send(Command::Seek(target));
    }

    /// p, m and the arrows in the media key context.
    pub fn key(&mut self, key: MediaKey, cx: &mut Context<Self>) {
        match key {
            MediaKey::TogglePlay => self.toggle(cx),
            MediaKey::Mute => MediaSound::update(cx, |sound| sound.toggle()),
            MediaKey::Seek(delta) => self.seek_to(self.position + delta),
        }
    }

    pub fn strip_view(&self, framed: bool, cx: &App) -> StripView {
        let sound = MediaSound::get(cx);
        StripView {
            playing: self.state.playback == Playback::Playing,
            muted: sound.muted,
            framed,
            position: self.position,
            duration: self.duration,
            scrub: self.scrub.active.then_some(self.scrub.fraction),
            level: sound.volume(),
        }
    }

    /// One strip input, as MediaStrip and MediaVolume handle it.
    pub fn strip_input(&mut self, input: StripInput, _: &mut Window, cx: &mut Context<Self>) {
        match input {
            StripInput::Toggle => self.toggle(cx),
            StripInput::Mute => MediaSound::update(cx, |sound| sound.toggle()),
            StripInput::ScrubPress(fraction) => self.scrub.press(fraction, 1.),
            StripInput::ScrubMove(fraction) => self.scrub.moved(fraction, 1.),
            StripInput::ScrubRelease => {
                if let Some(target) = self.scrub.release(self.duration) {
                    self.seek_to(target);
                }
            }
            StripInput::Wheel { up } => {
                if let Some(target) = wheel_seek(self.position, self.duration, up) {
                    self.seek_to(target);
                }
            }
            StripInput::LevelPress(level) => {
                MediaSound::update(cx, |sound| self.level.press(sound, level.into(), 100.))
            }
            StripInput::LevelMove(level) => MediaSound::update(cx, |sound| {
                self.level.moved(sound, level.into(), 100., true)
            }),
            StripInput::LevelRelease => {
                let kept = MediaSound::update(cx, |sound| self.level.release(sound));
                MediaSound::kept(kept, cx);
            }
            StripInput::Touched => return,
        }
        cx.notify();
    }
}
