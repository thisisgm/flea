//! The decoder helper's wire: fixed headers, bounded frames and one-line events.

/// Every record opens with 16 bytes: a tag and three little-endian words.
pub const HEADER: usize = 16;
pub const MAX_WIDTH: u32 = 3840;
pub const MAX_HEIGHT: u32 = 2160;
/// An event line is short; anything longer is a corrupt stream.
pub const MAX_EVENT: u32 = 4096;

/// One record's header as read off the helper's stdout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    /// A BGRA picture of exactly `width * height * 4` bytes follows.
    Frame { width: u32, height: u32 },
    /// A UTF-8 event line of `length` bytes follows.
    Event { length: u32 },
}

impl Record {
    /// The payload bytes that follow this header.
    pub fn length(&self) -> usize {
        match *self {
            Self::Frame { width, height } => width as usize * height as usize * 4,
            Self::Event { length } => length as usize,
        }
    }
}

fn word(header: &[u8; HEADER], at: usize) -> u32 {
    u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
}

/// Validates a header before anything is allocated for its payload.
pub fn parse_header(header: &[u8; HEADER]) -> Result<Record, &'static str> {
    let (a, b, length) = (word(header, 4), word(header, 8), word(header, 12));
    match &header[..4] {
        b"FRM1" => {
            if a == 0 || b == 0 || a > MAX_WIDTH || b > MAX_HEIGHT {
                return Err("frame size out of bounds");
            }
            let record = Record::Frame {
                width: a,
                height: b,
            };
            if record.length() != length as usize {
                return Err("frame length mismatch");
            }
            Ok(record)
        }
        b"EVT1" if a == 0 && b == 0 && length > 0 && length <= MAX_EVENT => {
            Ok(Record::Event { length })
        }
        b"EVT1" => Err("event length out of bounds"),
        _ => Err("unknown record"),
    }
}

/// The header a helper writes before a payload.
pub fn header(record: Record) -> [u8; HEADER] {
    let (tag, a, b) = match record {
        Record::Frame { width, height } => (b"FRM1", width, height),
        Record::Event { .. } => (b"EVT1", 0, 0),
    };
    let mut out = [0; HEADER];
    out[..4].copy_from_slice(tag);
    out[4..8].copy_from_slice(&a.to_le_bytes());
    out[8..12].copy_from_slice(&b.to_le_bytes());
    out[12..].copy_from_slice(&(record.length() as u32).to_le_bytes());
    out
}

/// Why playback failed, by category; the decoder's own words stay a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No decoder, demuxer or plugin for the stream.
    Unsupported,
    /// The file could not be opened or read.
    Unreadable,
    /// The library or the sandbox could not start.
    Helper,
}

impl Failure {
    pub fn word(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Unreadable => "unreadable",
            Self::Helper => "helper",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "unsupported" => Self::Unsupported,
            "unreadable" => Self::Unreadable,
            "helper" => Self::Helper,
            _ => return None,
        })
    }
}

/// What a helper reports about its player, one line each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The source opened: whether it carries a picture, and its length.
    Loaded {
        video: bool,
        duration_ms: i64,
    },
    Playing,
    Paused,
    Stopped,
    Position(i64),
    /// A seek landed here.
    Seeked(i64),
    Ended,
    Failed(Failure),
}

impl Event {
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_ascii_whitespace();
        let tag = words.next()?;
        let mut number = || words.next()?.parse::<i64>().ok().filter(|ms| *ms >= 0);
        let event = match tag {
            "loaded" => {
                let video = number()?;
                Self::Loaded {
                    video: video == 1,
                    duration_ms: number()?,
                }
            }
            "playing" => Self::Playing,
            "paused" => Self::Paused,
            "stopped" => Self::Stopped,
            "position" => Self::Position(number()?),
            "seeked" => Self::Seeked(number()?),
            "ended" => Self::Ended,
            "failed" => Self::Failed(Failure::parse(words.next()?)?),
            _ => return None,
        };
        Some(event)
    }

    pub fn render(&self) -> String {
        match *self {
            Self::Loaded { video, duration_ms } => {
                format!("loaded {} {duration_ms}", u8::from(video))
            }
            Self::Playing => "playing".into(),
            Self::Paused => "paused".into(),
            Self::Stopped => "stopped".into(),
            Self::Position(ms) => format!("position {ms}"),
            Self::Seeked(ms) => format!("seeked {ms}"),
            Self::Ended => "ended".into(),
            Self::Failed(failure) => format!("failed {}", failure.word()),
        }
    }
}

/// A command the GUI writes on the helper's stdin, one line each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Stop,
    /// An absolute seek in milliseconds.
    Seek(i64),
    Mute(bool),
    /// Linear gain, 0 to 1.
    Gain(f64),
    /// The current picture box in device pixels, without restarting playback.
    Resize(u32, u32),
    Quit,
}

impl Command {
    pub fn render(&self) -> String {
        match *self {
            Self::Play => "play".into(),
            Self::Pause => "pause".into(),
            Self::Stop => "stop".into(),
            Self::Seek(ms) => format!("seek {}", ms.max(0)),
            Self::Mute(muted) => format!("mute {}", u8::from(muted)),
            Self::Gain(gain) => format!("gain {:.6}", gain.clamp(0., 1.)),
            Self::Resize(w, h) => format!("resize {w} {h}"),
            Self::Quit => "quit".into(),
        }
    }

    /// The helper's side; a malformed line is ignored rather than guessed at.
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_ascii_whitespace();
        let command = match (words.next()?, words.next()) {
            ("play", None) => Self::Play,
            ("pause", None) => Self::Pause,
            ("stop", None) => Self::Stop,
            ("quit", None) => Self::Quit,
            ("seek", Some(ms)) => Self::Seek(ms.parse::<i64>().ok().filter(|ms| *ms >= 0)?),
            ("mute", Some("0")) => Self::Mute(false),
            ("mute", Some("1")) => Self::Mute(true),
            ("resize", Some(width)) => Self::Resize(
                width
                    .parse::<u32>()
                    .ok()
                    .filter(|w| (1..=MAX_WIDTH).contains(w))?,
                words
                    .next()?
                    .parse::<u32>()
                    .ok()
                    .filter(|h| (1..=MAX_HEIGHT).contains(h))?,
            ),
            ("gain", Some(value)) => {
                let gain = value.parse::<f64>().ok().filter(|gain| gain.is_finite())?;
                Self::Gain(gain.clamp(0., 1.))
            }
            _ => return None,
        };
        words.next().is_none().then_some(command)
    }
}
