//! The HTML preview's rules (ui/PreviewHtml.qml): the helper's still image of the page, asked once the frame's width
//! has settled, and the shipped code view for Source. Nothing here parses the page: the helper renders it out of process.
use super::code_view::{Answer, BYTE_GATE};
use super::refusal::Refusal;

/// Repeated width changes restart this settle, so no helper runs while the frame is being resized.
pub const SETTLE_MS: u64 = 160;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Rendered,
    Source,
}

impl View {
    /// R flips the open Quick Look; the column never stores a view.
    pub fn flipped(self) -> Self {
        match self {
            View::Rendered => View::Source,
            View::Source => View::Rendered,
        }
    }
}

/// The page image's own load, after the reply named its file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Picture {
    #[default]
    None,
    Loading,
    Ready,
    Error,
}

/// What the view sends the backend's `html` slot.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Ask {
        slot: String,
        path: String,
        width: u32,
        height: u32,
    },
    Cancel {
        slot: String,
        width: u32,
    },
}

/// One `html` reply, parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub token: u64,
    pub slot: String,
    pub state: String,
    pub file: String,
    pub total: usize,
    pub width: u32,
    pub title: String,
}

#[derive(Default)]
pub struct Html {
    pub active: bool,
    pub path: String,
    pub size: u64,
    pub max_bytes: u64,
    /// "column" or "look"; Quick Look asks for the viewport's height as well.
    pub slot: String,
    pub view: View,
    pub device_width: u32,
    pub device_height: u32,
    pub token: Option<u64>,
    pub pending: bool,
    pub read_failed: bool,
    pub has_drawn: bool,
    pub image_token: u64,
    pub total: usize,
    pub image_file: String,
    pub page_title: String,
    pub rendered_width: u32,
    pub zoom: f64,
    /// Source's code view is built the first time Source shows and kept after.
    pub source_built: bool,
    pub picture: Picture,
    settle_at: Option<u64>,
    commands: Vec<Command>,
    /// Asks sent, for the count tests.
    pub asks: usize,
}

impl Html {
    pub fn new(slot: &str) -> Self {
        Html {
            slot: slot.to_string(),
            max_bytes: BYTE_GATE,
            zoom: 1.,
            ..Html::default()
        }
    }

    pub fn source_view(&self) -> bool {
        self.view == View::Source
    }

    pub fn over_size(&self) -> bool {
        self.size > self.max_bytes
    }

    /// The frame's device size: Quick Look asks the viewport's height at that width, the column keeps the helper's own.
    pub fn set_frame(&mut self, width: f64, height: f64, scale: f64, now: u64) {
        let device_width = (width * scale).round().max(1.) as u32;
        let device_height = if self.slot == "look" {
            (height * f64::from(device_width) / width.max(1.))
                .ceil()
                .max(1.) as u32
        } else {
            0
        };
        if (device_width, device_height) != (self.device_width, self.device_height) {
            self.device_width = device_width;
            self.device_height = device_height;
            self.schedule(now);
        }
    }

    /// A new subject drops the old page, its line count and zoom; the column's next ask waits for the settle.
    pub fn set_path(&mut self, path: &str, size: u64, now: u64) {
        self.cancel();
        self.path = path.to_string();
        self.size = size;
        self.has_drawn = false;
        self.image_file.clear();
        self.picture = Picture::None;
        self.total = 0;
        self.read_failed = false;
        self.zoom = 1.;
        self.schedule(now);
    }

    pub fn set_active(&mut self, active: bool, now: u64) {
        self.active = active;
        self.schedule(now);
    }

    pub fn set_view(&mut self, view: View, now: u64) {
        if self.view == view {
            return;
        }
        self.view = view;
        if self.source_view() {
            self.source_built = true;
        }
        self.read_failed = false;
        self.schedule(now);
    }

    fn cancel(&mut self) {
        self.settle_at = None;
        if self.pending {
            self.commands.push(Command::Cancel {
                slot: self.slot.clone(),
                width: self.device_width,
            });
        }
        self.token = None;
        self.pending = false;
    }

    /// Any change that can alter the page restarts the settle; Source and an oversize page ask nothing.
    pub fn schedule(&mut self, now: u64) {
        if !self.active || self.source_view() || self.over_size() {
            self.cancel();
            return;
        }
        self.settle_at = Some(now + SETTLE_MS);
    }

    pub fn deadline(&self) -> Option<u64> {
        self.settle_at
    }

    /// The settle ran out: the ask goes out with the frame's device size.
    pub fn tick(&mut self, now: u64) {
        if self.settle_at.is_some_and(|at| at <= now) {
            self.settle_at = None;
            self.ask();
        }
    }

    /// Asks at once; a reload of the same path keeps the drawn page up with no loading mark.
    pub fn ask(&mut self) {
        if !self.active || self.source_view() || self.over_size() || self.device_width == 0 {
            return;
        }
        self.read_failed = false;
        self.pending = true;
        self.asks += 1;
        self.commands.push(Command::Ask {
            slot: self.slot.clone(),
            path: self.path.clone(),
            width: self.device_width,
            height: self.device_height,
        });
    }

    /// The host's token for the ask just sent.
    pub fn sent(&mut self, token: u64) {
        self.token = Some(token);
    }

    /// A host with no html route: the frame reads as unreadable rather than loading forever.
    pub fn no_backend(&mut self) {
        self.read_failed = true;
        self.pending = false;
    }

    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    /// One reply: true when it was this view's latest ask; a stale, closed or Source-time reply is dropped.
    pub fn reply(&mut self, reply: &Reply) -> bool {
        if !self.active
            || self.source_view()
            || reply.slot != self.slot
            || Some(reply.token) != self.token
        {
            return false;
        }
        self.pending = false;
        self.read_failed = reply.state != "ready";
        if !self.read_failed {
            self.total = reply.total;
            self.rendered_width = reply.width;
            self.page_title = reply.title.clone();
            self.image_file = reply.file.clone();
            self.image_token = reply.token;
            self.picture = Picture::Loading;
        }
        true
    }

    /// The decode of the named file finished; a failed decode is the unreadable frame.
    pub fn picture_loaded(&mut self, token: u64, ok: bool) {
        if token != self.image_token || self.image_file.is_empty() {
            return;
        }
        self.picture = if ok { Picture::Ready } else { Picture::Error };
        if ok {
            self.has_drawn = true;
        }
    }

    /// The refusal for the view in effect; Source reads its code view's answer.
    pub fn refusal(&self, source: Answer) -> Option<Refusal> {
        if self.source_view() {
            super::code_view::refusal(self.size, self.max_bytes, source, self.read_failed)
        } else if self.over_size() {
            Some(Refusal::TooLarge)
        } else if self.read_failed || self.picture == Picture::Error {
            Some(Refusal::Unreadable)
        } else {
            None
        }
    }

    pub fn content_ready(&self, source_settled: bool) -> bool {
        if self.source_view() {
            source_settled
        } else {
            !self.pending && self.picture == Picture::Ready
        }
    }

    /// The first screen is whole: a refusal, the page drawn, or Source's lines in view held.
    pub fn first_screen(&self, source: Answer, source_first_screen: bool) -> bool {
        self.refusal(source).is_some()
            || if self.source_view() {
                source_first_screen
            } else {
                self.content_ready(false)
            }
    }

    /// The loading mark: Source until its first screen, the page only before its first drawing, so a re-ask keeps the old page.
    pub fn loading(&self, source: Answer, source_first_screen: bool) -> bool {
        self.active
            && !self.first_screen(source, source_first_screen)
            && (self.source_view() || !self.has_drawn)
    }

    pub fn zoom_by(&mut self, steps: i64) {
        if !self.source_view() {
            self.zoom = super::viewer::zoom_step(self.zoom, steps);
        }
    }

    pub fn zoom_fit(&mut self) {
        self.zoom = 1.;
    }
}

/// The backend's html ask line; the column leaves the height to the helper, as ui/Backend.qml does.
pub fn ask_line(token: u64, slot: &str, path: &str, width: u32, height: u32) -> String {
    // The backend reads "height":0 as a 1 px viewport, which decodes a short page only as tall as its text.
    let height = if height > 0 {
        format!(r#","height":{height}"#)
    } else {
        String::new()
    };
    format!(
        r#"{{"c":"html","token":{token},"slot":"{}","path":"{}","width":{width}{height}}}"#,
        crate::json::escape(slot),
        crate::json::escape(path)
    )
}

/// The page image's drawn size: as wide as the frame times the zoom, its height kept to the decode's proportion.
pub fn picture_size(frame_width: f64, zoom: f64, decoded: (u32, u32)) -> (f64, f64) {
    let width = frame_width * zoom;
    let height = if decoded.0 > 0 {
        f64::from(decoded.1) * width / f64::from(decoded.0)
    } else {
        0.
    };
    (width, height)
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
