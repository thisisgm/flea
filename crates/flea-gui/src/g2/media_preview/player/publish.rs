//! What a media preview tells the desktop: the idle inhibitor and the MPRIS state lines.
use super::MediaPlayer;
use crate::g2::media_preview::mpris::Mpris;
use flea::ui_logic_media_preview::session::{LineState, Remote};
use gpui::Context;

impl MediaPlayer {
    pub(super) fn set_awake(&mut self, awake: bool, cx: &mut Context<Self>) {
        if awake != self.awake {
            self.awake = awake;
            if let Some(inhibit) = self.inhibit.clone() {
                inhibit(awake, cx);
            }
        }
    }

    /// Sends the MPRIS state line, starting the helper on the first play; ticks of the clock send nothing.
    pub(super) fn publish(&mut self, cx: &mut Context<Self>) {
        let Some(subject) = self.subject.as_ref() else {
            return;
        };
        let state = LineState {
            playback: self.state.playback,
            path: subject.path.to_string_lossy().into_owned(),
            length: self.duration,
            position: self.position,
            seeked: std::mem::take(&mut self.seeked),
        };
        let Some(line) = self.session.line(&state) else {
            return;
        };
        match self.mpris.as_ref() {
            Some(mpris) => mpris.send(line),
            None => {
                let (sender, received) = async_channel::unbounded::<Remote>();
                let generation = self.generation;
                let task = cx.spawn(async move |this, cx| {
                    while let Ok(remote) = received.recv().await {
                        let _ = this.update(cx, |this, cx| {
                            if this.generation == generation {
                                this.remote(remote, cx);
                            }
                        });
                    }
                });
                self.mpris = Some(Mpris::start(
                    self.mpris_program.clone(),
                    line,
                    sender.clone(),
                ));
                self.remote = Some((sender, task));
            }
        }
    }
}
