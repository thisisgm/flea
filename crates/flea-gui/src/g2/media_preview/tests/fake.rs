//! A scripted backend: it records every request and command, and hands the test each decoder's feed end.
use crate::g2::media_preview::engine::{feed, Backend, Feed, FeedSender, Playback, Request};
use flea::ui_logic_media_preview::frame::Command;
use gpui::RenderImage;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub struct Script {
    pub requests: RefCell<Vec<Request>>,
    pub commands: Rc<RefCell<Vec<Command>>>,
    pub senders: RefCell<Vec<FeedSender>>,
    pub dropped: Rc<Cell<usize>>,
    pub refuse: Cell<bool>,
}

impl Script {
    pub fn sender(&self) -> FeedSender {
        self.senders
            .borrow()
            .last()
            .cloned()
            .expect("a decoder was started")
    }

    pub fn take_commands(&self) -> Vec<Command> {
        std::mem::take(&mut *self.commands.borrow_mut())
    }
}

pub struct Fake(pub Rc<Script>);

struct Decoder {
    commands: Rc<RefCell<Vec<Command>>>,
    dropped: Rc<Cell<usize>>,
}

impl Playback for Decoder {
    fn send(&mut self, command: Command) {
        self.commands.borrow_mut().push(command);
    }

    fn pids(&self) -> Vec<u32> {
        Vec::new()
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}

impl Backend for Fake {
    fn start(&self, request: &Request) -> Result<(Box<dyn Playback>, Feed), String> {
        if self.0.refuse.get() {
            return Err("refused by the script".into());
        }
        self.0.requests.borrow_mut().push(request.clone());
        let (sender, feed) = feed();
        self.0.senders.borrow_mut().push(sender);
        let decoder = Decoder {
            commands: self.0.commands.clone(),
            dropped: self.0.dropped.clone(),
        };
        Ok((Box::new(decoder), feed))
    }
}

/// A small opaque picture whose first pixel carries `shade`, so two pictures tell apart.
pub fn picture(shade: u8) -> Arc<RenderImage> {
    let mut pixels = vec![255_u8; 2 * 2 * 4];
    pixels[0] = shade;
    let buffer = image::RgbaImage::from_raw(2, 2, pixels).expect("2x2 BGRA");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

const MARKER: &str = ".flea-pv3-test-root";

/// A fresh mktemp root holding the marker, removed only after the marker is checked.
pub struct Root(pub PathBuf);

impl Root {
    pub fn new() -> Self {
        let output = std::process::Command::new("/usr/bin/mktemp")
            .args(["-d", "-t", "flea-pv3.XXXXXX"])
            .output()
            .expect("mktemp");
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        assert!(
            output.status.success() && path.is_absolute() && path.file_name().is_some(),
            "{path:?}"
        );
        std::fs::write(path.join(MARKER), b"pv3\n").expect("marker");
        Self(path)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        if self.0.is_absolute() && self.0.as_os_str().len() > 1 && self.0.join(MARKER).is_file() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
