//! The backend seam: what the player asks of a decoder, and the feed a decoder answers on.
use flea::ui_logic_media_preview::frame::{Command, Event};
use gpui::RenderImage;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// One subject to decode, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub path: PathBuf,
    pub box_w: u32,
    pub box_h: u32,
    pub audio: bool,
    pub autoplay: bool,
}

/// What a running decoder reports; frames ride the slot so a slow paint drops old pictures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Incoming {
    Event(Event),
    Frame,
    Closed,
}

/// The newest decoded picture, replaced and never queued.
#[derive(Default)]
pub struct FrameSlot {
    latest: Mutex<Option<Arc<RenderImage>>>,
    pending: AtomicBool,
}

impl FrameSlot {
    /// Takes the newest picture, if one arrived since the last take.
    pub fn take(&self) -> Option<Arc<RenderImage>> {
        self.pending.store(false, Ordering::Release);
        self.latest.lock().ok()?.take()
    }
}

/// The player's end of a decoder.
pub struct Feed {
    pub incoming: async_channel::Receiver<Incoming>,
    pub frames: Arc<FrameSlot>,
}

/// The decoder's end of a feed.
#[derive(Clone)]
pub struct FeedSender {
    incoming: async_channel::Sender<Incoming>,
    frames: Arc<FrameSlot>,
}

/// Records the UI thread has yet to take; past this the helper's reader waits, so a flood fills the pipe, not the heap.
pub const QUEUE: usize = 256;

/// Every send waits for room; only the decoder's own threads send, never the UI thread.
impl FeedSender {
    pub fn event(&self, event: Event) -> bool {
        self.incoming.send_blocking(Incoming::Event(event)).is_ok()
    }

    /// Stores a picture, waking the player only when it had none waiting.
    pub fn frame(&self, image: Arc<RenderImage>) -> bool {
        if let Ok(mut latest) = self.frames.latest.lock() {
            *latest = Some(image);
        }
        if self.frames.pending.swap(true, Ordering::AcqRel) {
            return !self.incoming.is_closed();
        }
        self.incoming.send_blocking(Incoming::Frame).is_ok()
    }

    pub fn closed(&self) {
        let _ = self.incoming.send_blocking(Incoming::Closed);
    }
}

/// A connected pair of feed ends.
pub fn feed() -> (FeedSender, Feed) {
    let (incoming, receiver) = async_channel::bounded(QUEUE);
    let frames = Arc::new(FrameSlot::default());
    (
        FeedSender {
            incoming,
            frames: frames.clone(),
        },
        Feed {
            incoming: receiver,
            frames,
        },
    )
}

/// A running decoder; dropping it ends the decoder and everything it started.
pub trait Playback {
    fn send(&mut self, command: Command);
    /// The process ids a memory probe should walk, empty for an in-process decoder.
    fn pids(&self) -> Vec<u32>;
}

/// Starts decoders; the sandboxed helper in production, a scripted fake under test.
pub trait Backend {
    fn start(&self, request: &Request) -> Result<(Box<dyn Playback>, Feed), String>;
}
