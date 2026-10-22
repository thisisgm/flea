//! One media preview's MPRIS session: the state lines it writes and the commands its helper prints.
use super::player::Playback;
use crate::json::escape;

/// One preview's MPRIS session: silent until the first play, then one line per status, track or seek change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub active: bool,
}

impl Session {
    /// The first play makes this an MPRIS player.
    pub fn playback_changed(&mut self, playback: Playback) {
        if playback == Playback::Playing {
            self.active = true;
        }
    }

    /// The state line to send, none before the first play.
    pub fn line(&self, state: &LineState) -> Option<String> {
        self.active.then(|| state.render())
    }
}

/// The fields one MPRIS state line carries, times in milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineState {
    pub playback: Playback,
    pub path: String,
    pub length: i64,
    pub position: i64,
    pub seeked: bool,
}

impl LineState {
    pub fn render(&self) -> String {
        let status = match self.playback {
            Playback::Playing => "Playing",
            Playback::Paused => "Paused",
            Playback::Stopped => "Stopped",
        };
        format!(
            "{{\"status\":\"{status}\",\"title\":\"{}\",\"url\":\"{}\",\"length\":{},\"position\":{},\"seeked\":{}}}",
            escape(super::file_name(&self.path)),
            escape(&super::file_uri(&self.path)),
            self.length,
            self.position,
            self.seeked
        )
    }
}

/// A command the MPRIS helper prints: Seek is relative, SetPosition absolute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remote {
    Play,
    Pause,
    PlayPause,
    Stop,
    Seek(i64),
    SetPosition(i64),
}

impl Remote {
    /// One helper line, none for an unknown word or a number that does not parse.
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split(' ');
        let word = words.next()?;
        let number = || {
            words
                .clone()
                .next()
                .and_then(|value| value.trim().parse::<i64>().ok())
        };
        Some(match word.trim() {
            "Play" => Self::Play,
            "Pause" => Self::Pause,
            "PlayPause" => Self::PlayPause,
            "Stop" => Self::Stop,
            "Seek" => Self::Seek(number()?),
            "SetPosition" => Self::SetPosition(number()?),
            _ => return None,
        })
    }
}
