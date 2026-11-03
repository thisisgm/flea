//! One media subject's player: it drives a decoder through the backend seam and projects status, frame and strip.
use super::engine::{Backend, FrameSlot, Incoming, Playback as Decoder, Request};
use super::mpris::{self, Mpris};
use super::sound::MediaSound;
use super::view::Shown;
use flea::ui_logic_pv3::frame::{Command, Event, Failure};
use flea::ui_logic_pv3::player::{Media, Playback, Player, Status};
use flea::ui_logic_pv3::session::{Remote, Session};
use flea::ui_logic_pv3::strip::{Level, Scrub};
use gpui::{App, Context, RenderImage, Task};

mod atlas;
mod controls;
mod publish;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// What the host hands the player for one subject.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub path: PathBuf,
    pub video: bool,
    /// The picture area in device pixels, so the helper never decodes past what is drawn.
    pub box_w: u32,
    pub box_h: u32,
    /// Quick Look starts playing; the column's explicit Play does too, since its press built the player.
    pub autoplay: bool,
}

/// Turns the idle inhibitor on or off; a stand-in until the window's inhibitor seam lands.
pub type Inhibit = Rc<dyn Fn(bool, &mut App)>;

pub struct MediaPlayer {
    backend: Rc<dyn Backend>,
    pub subject: Option<Subject>,
    decoder: Option<Box<dyn Decoder>>,
    frames: Option<Arc<FrameSlot>>,
    /// The newest picture; the previous subject's stays up until the next one's first frame replaces it.
    pub frame: Option<Arc<RenderImage>>,
    /// What the picture area uploaded; its paint frees a replaced picture, and close, hide and drop free the rest.
    pub painted: Shown,
    /// Bumped per subject, so a late record from the one before is ignored.
    generation: u64,
    /// Generation of the shown frame, so a stale picture is known as one.
    pub frame_generation: u64,
    pub state: Player,
    pub position: i64,
    pub duration: i64,
    /// Inline video returns to its poster; Quick Look holds its final frame and clock.
    pub rewind_on_end: bool,
    seeked: bool,
    session: Session,
    mpris: Option<Mpris>,
    remote: Option<(async_channel::Sender<Remote>, Task<()>)>,
    inhibit: Option<Inhibit>,
    awake: bool,
    scrub: Scrub,
    level: Level,
    feed: Option<Task<()>>,
    /// Path of the MPRIS helper, the environment's unless a test names one.
    pub mpris_program: PathBuf,
}

impl MediaPlayer {
    pub fn new(backend: Rc<dyn Backend>, inhibit: Option<Inhibit>, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<MediaSound>(|this, cx| this.follow_sound(cx))
            .detach();
        // A host may drop the player without closing it; the decoder goes with its handle, the rest goes here.
        cx.on_release(|this, cx| {
            if let (true, Some(inhibit)) = (this.awake, this.inhibit.as_ref()) {
                inhibit(false, cx);
            }
            atlas::release_painted(&this.painted, this.frame.as_ref(), cx);
        })
        .detach();
        Self {
            backend,
            subject: None,
            decoder: None,
            frames: None,
            frame: None,
            painted: Shown::default(),
            generation: 0,
            frame_generation: 0,
            state: Player::default(),
            position: 0,
            duration: 0,
            rewind_on_end: false,
            seeked: false,
            session: Session::default(),
            mpris: None,
            remote: None,
            inhibit,
            awake: false,
            scrub: Scrub::default(),
            level: Level::default(),
            feed: None,
            mpris_program: mpris::program(std::env::var_os(mpris::HELPER_ENV)),
        }
    }

    /// The current subject's number; a frame from an earlier one has a lower `frame_generation`.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn status(&self) -> Status {
        self.state.status()
    }

    /// Starts a subject, keeping the last picture up until the new one draws.
    pub fn open(&mut self, subject: Subject, cx: &mut Context<Self>) {
        if self.subject.as_ref() == Some(&subject) {
            return;
        }
        self.end(cx);
        self.generation += 1;
        let request = Request {
            path: subject.path.clone(),
            box_w: subject.box_w.max(1),
            box_h: subject.box_h.max(1),
            audio: true,
            autoplay: subject.autoplay,
        };
        self.state = Player {
            media: Media::Loading,
            has_video: subject.video,
            ..Player::default()
        };
        self.subject = Some(subject);
        match self.backend.start(&request) {
            Ok((mut decoder, feed)) => {
                let sound = MediaSound::get(cx);
                decoder.send(Command::Mute(sound.muted));
                decoder.send(Command::Gain(sound.gain()));
                self.decoder = Some(decoder);
                self.frames = Some(feed.frames.clone());
                let generation = self.generation;
                self.feed = Some(cx.spawn(async move |this, cx| {
                    while let Ok(incoming) = feed.incoming.recv().await {
                        let alive =
                            this.update(cx, |this, cx| this.receive(generation, incoming, cx));
                        if !matches!(alive, Ok(true)) {
                            break;
                        }
                    }
                }));
            }
            Err(error) => {
                eprintln!("flea-gui: {error}");
                self.state.failed = true;
            }
        }
        cx.notify();
    }

    /// Resizes the existing decoder while preserving its play state, position and held frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (
            width.clamp(1, flea::ui_logic_pv3::frame::MAX_WIDTH),
            height.clamp(1, flea::ui_logic_pv3::frame::MAX_HEIGHT),
        );
        let Some(subject) = self.subject.as_mut().filter(|subject| subject.video) else {
            return;
        };
        if (subject.box_w, subject.box_h) == (width, height) {
            return;
        }
        subject.box_w = width;
        subject.box_h = height;
        self.send(Command::Resize(width, height));
    }

    /// Ends the subject: the decoder, the MPRIS player and the inhibitor go; the picture leaves the atlas.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.end(cx);
        self.subject = None;
        let frame = self.frame.take();
        atlas::release_painted(&self.painted, frame.as_ref(), cx);
        self.state = Player::default();
        cx.notify();
    }

    fn end(&mut self, cx: &mut Context<Self>) {
        self.feed = None;
        self.decoder = None;
        self.frames = None;
        self.mpris = None;
        self.remote = None;
        self.session = Session::default();
        self.position = 0;
        self.duration = 0;
        if self.scrub.active {
            self.scrub = Scrub::default();
        }
        if self.level.pressing {
            MediaSound::update(cx, |sound| self.level.destroyed(sound));
        }
        self.set_awake(false, cx);
    }

    /// The process ids behind this player, for a memory probe.
    pub fn pids(&self) -> Vec<u32> {
        self.decoder
            .as_ref()
            .map_or_else(Vec::new, |decoder| decoder.pids())
    }

    fn send(&mut self, command: Command) {
        if let Some(decoder) = self.decoder.as_mut() {
            decoder.send(command);
        }
    }

    /// Pushes the shared sound to this decoder; the global's observer calls it on every change.
    pub fn follow_sound(&mut self, cx: &mut Context<Self>) {
        let sound = MediaSound::get(cx);
        self.send(Command::Mute(sound.muted));
        self.send(Command::Gain(sound.gain()));
        cx.notify();
    }

    fn receive(&mut self, generation: u64, incoming: Incoming, cx: &mut Context<Self>) -> bool {
        if generation != self.generation {
            return false;
        }
        match incoming {
            Incoming::Frame => {
                if let Some(frame) = self.frames.as_ref().and_then(|slot| slot.take()) {
                    self.replace_frame(frame, cx);
                    self.frame_generation = generation;
                    if self.state.media == Media::Loading {
                        self.state.media = Media::Buffered;
                    }
                }
            }
            Incoming::Event(event) => self.event(event, cx),
            Incoming::Closed => {
                // A helper that ends while its decoder is still wanted died, so the player fails rather than plays on.
                if self.decoder.is_some() {
                    self.event(Event::Failed(Failure::Helper), cx);
                }
                cx.notify();
                return false;
            }
        }
        cx.notify();
        true
    }

    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        let mut publish = true;
        match event {
            Event::Loaded { video, duration_ms } => {
                self.state.has_video = video;
                self.duration = duration_ms;
                if self.state.media == Media::Loading {
                    self.state.media = Media::Loaded;
                }
            }
            Event::Playing => {
                self.state.playback = Playback::Playing;
                // The helper rewound a Play at the end, so the clip is no longer at its end.
                if self.state.media == Media::Ended {
                    self.state.media = Media::Buffered;
                }
            }
            Event::Paused => self.state.playback = Playback::Paused,
            Event::Stopped => {
                self.state.playback = Playback::Stopped;
                self.position = 0;
            }
            Event::Position(ms) => {
                if !(self.state.has_video && self.state.media == Media::Ended) {
                    self.position = ms;
                }
                publish = false;
            }
            Event::Seeked(ms) => {
                self.position = ms;
                self.seeked = true;
                // A seek leaves the end, so the frame it lands on draws again.
                if self.state.media == Media::Ended {
                    self.state.media = Media::Buffered;
                }
            }
            Event::Ended => {
                self.state.media = Media::Ended;
                self.state.playback = Playback::Stopped;
                if self.state.has_video && self.rewind_on_end {
                    self.position = 0;
                    // Stop pauses and rewinds the decoder so the poster replaces the held picture.
                    self.send(Command::Stop);
                } else if self.duration > 0 {
                    self.position = self.duration;
                }
            }
            Event::Failed(_) => {
                self.state.failed = true;
                self.state.playback = Playback::Stopped;
                self.decoder = None;
            }
        }
        self.session.playback_changed(self.state.playback);
        let awake = self.state.keeps_awake();
        self.set_awake(awake, cx);
        if publish {
            self.publish(cx);
        }
    }
}
