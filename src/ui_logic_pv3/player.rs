//! What a media preview projects from its player: status, poster, idle hold and keys.

/// The Quick Look's relative seek, Left and Right.
pub const SEEK_MS: i64 = 5000;
/// One wheel click on the seek track.
pub const WHEEL_STEP_MS: i64 = 1000;
/// StatusBar.messageMs: how long the Quick Look strip stays over a video after the last contact.
pub const STRIP_HIDE_MS: u64 = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Playback {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// How far the player has got with its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Media {
    #[default]
    NoMedia,
    Loading,
    Buffering,
    Loaded,
    Buffered,
    Ended,
}

/// The one word a preview reports, a failure first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Failed,
    Playing,
    Paused,
    Loading,
    Stopped,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Loading => "loading",
            Self::Stopped => "stopped",
        }
    }
}

/// A player as the preview reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Player {
    pub playback: Playback,
    pub media: Media,
    pub failed: bool,
    pub has_video: bool,
}

impl Player {
    pub fn status(&self) -> Status {
        if self.failed {
            return Status::Failed;
        }
        match (self.playback, self.media) {
            (Playback::Playing, _) => Status::Playing,
            (Playback::Paused, _) => Status::Paused,
            (_, Media::Loading | Media::Buffering) => Status::Loading,
            _ => Status::Stopped,
        }
    }

    /// The play mark over a stopped video, gated on the media status so it never flashes before the first frame.
    pub fn poster(&self, video: bool) -> bool {
        video
            && !self.failed
            && self.playback != Playback::Playing
            && matches!(self.media, Media::Loaded | Media::Buffered | Media::Ended)
    }

    /// The screen stays awake exactly while a clip with a picture plays; audio and a failure never hold it.
    pub fn keeps_awake(&self) -> bool {
        self.has_video && !self.failed && self.playback == Playback::Playing
    }
}

/// An absolute seek clamped to the clip.
pub fn clamp_seek(ms: i64, duration: i64) -> i64 {
    ms.min(duration).max(0)
}

/// What a preview key does to a media preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKey {
    TogglePlay,
    Mute,
    Seek(i64),
}

/// PreviewKeys.act's media arms: each self-guards, so a still preview ignores them.
pub fn media_key(action: &str, is_media: bool) -> Option<MediaKey> {
    if !is_media {
        return None;
    }
    match action {
        "playPause" => Some(MediaKey::TogglePlay),
        "mute" => Some(MediaKey::Mute),
        "seekBack" => Some(MediaKey::Seek(-SEEK_MS)),
        "seekForward" => Some(MediaKey::Seek(SEEK_MS)),
        _ => None,
    }
}

/// What a press of the column strip's play mark does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnPress {
    Create,
    Toggle,
    Nothing,
}

/// The preview column's play intent: no player until play is pressed, and none past a subject change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnPlay {
    pub wants: bool,
    allowed: bool,
}

impl ColumnPlay {
    /// The column on screen, no overlay over it and a strip to drive it; losing any one ends the intent.
    pub fn set_allowed(&mut self, visible: bool, overlay_open: bool, strip: bool) {
        self.allowed = visible && !overlay_open && strip;
        if !self.allowed {
            self.wants = false;
        }
    }

    pub fn path_changed(&mut self) {
        self.wants = false;
    }

    pub fn press(&mut self, player_exists: bool) -> ColumnPress {
        if !self.wants {
            self.wants = true;
            ColumnPress::Create
        } else if player_exists {
            ColumnPress::Toggle
        } else {
            ColumnPress::Nothing
        }
    }

    /// Whether a player should exist now.
    pub fn player(&self) -> bool {
        self.wants && self.allowed
    }
}

/// The column strip's scale: the player's own once it has played, else the probe's.
pub fn column_duration(position: i64, player_duration: i64, probe_duration: i64) -> i64 {
    if position > 0 && player_duration > 0 {
        player_duration
    } else {
        probe_duration.max(0)
    }
}

/// The Quick Look strip over a video: shown on open, hidden a while after the last contact, held under a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripHide {
    pub shown: bool,
    deadline: Option<u64>,
    pub hide_ms: u64,
}

impl Default for StripHide {
    fn default() -> Self {
        Self {
            shown: true,
            deadline: None,
            hide_ms: STRIP_HIDE_MS,
        }
    }
}

impl StripHide {
    pub fn reveal(&mut self, now_ms: u64) {
        self.shown = true;
        self.deadline = Some(now_ms + self.hide_ms);
    }

    /// The timer's turn: a held seek or volume press restarts it, else the strip goes.
    pub fn tick(&mut self, now_ms: u64, holding: bool) {
        match self.deadline {
            Some(due) if now_ms >= due && holding => self.deadline = Some(now_ms + self.hide_ms),
            Some(due) if now_ms >= due => {
                self.shown = false;
                self.deadline = None;
            }
            _ => {}
        }
    }

    pub fn stop(&mut self) {
        self.deadline = None;
    }

    /// When the timer next wants a turn.
    pub fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    /// Audio has nothing else to look at, so its strip never hides.
    pub fn visible(&self, audio: bool) -> bool {
        audio || self.shown
    }
}
