//! Scripted decoder behind the real MediaPlayer entity; Drop proves lifetime, not only UI state.
use flea::ui_logic_pv3::frame::{Command, Event};
use flea_gui::g2::pv3::engine::{self, Backend, FeedSender, Playback, Request};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[derive(Default)]
pub struct Decoder {
    pub started: Arc<AtomicUsize>,
    pub ended: Arc<AtomicUsize>,
}
struct Playing {
    ended: Arc<AtomicUsize>,
    _sender: FeedSender,
}
impl Backend for Decoder {
    fn start(&self, _: &Request) -> Result<(Box<dyn Playback>, engine::Feed), String> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let (sender, feed) = engine::feed();
        sender.event(Event::Loaded {
            video: true,
            duration_ms: 2000,
        });
        sender.frame(Arc::new(
            gpui::RenderImage::from_bgra(2, 2, vec![128; 16]).unwrap(),
        ));
        sender.event(Event::Playing);
        Ok((
            Box::new(Playing {
                ended: self.ended.clone(),
                _sender: sender,
            }),
            feed,
        ))
    }
}
impl Playback for Playing {
    fn send(&mut self, _: Command) {}
    fn pids(&self) -> Vec<u32> {
        Vec::new()
    }
}
impl Drop for Playing {
    fn drop(&mut self) {
        self.ended.fetch_add(1, Ordering::SeqCst);
    }
}
