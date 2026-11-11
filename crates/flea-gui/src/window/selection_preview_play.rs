//! The column's media player: the decoder seam and the play toggle the strip and the keymap share.
use super::*;
use crate::g2::media_preview::{self, strip::StripInput};
use gpui::{div, prelude::*, AppContext, IntoElement, Window};

impl SelectionPreview {
    /// A host may provide the decoder seam while this owner keeps playback lifetime.
    pub fn set_media_backend(&mut self, backend: Rc<dyn media_preview::engine::Backend>) {
        self.media_backend = Some(backend);
    }

    /// The video's frame; a click on its picture toggles play through the same entry point as the strip and the keymap.
    pub(crate) fn video_frame(
        failed: bool,
        element: media_preview::view::MediaElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("column-media-frame")
            .size_full()
            .when(failed, |d| {
                d.debug_selector(|| "column-media-failed".into())
            })
            .on_click(cx.listener(|view, event, window, cx| {
                if media_preview::surface::frame_toggles(event) {
                    view.play(StripInput::Toggle, window, cx);
                }
            }))
            .child(element)
    }

    /// The transport and preview keymap share this entry point.
    pub fn play_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.play(StripInput::Toggle, window, cx);
    }

    pub(crate) fn play(&mut self, input: StripInput, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible || self.pending.is_some() {
            return;
        }
        let Some(frame) = self.drawn.as_mut().filter(|f| !f.manual) else {
            return;
        };
        let Mounted::Media(player) = &mut frame.mounted else {
            return;
        };
        if player.is_none() && input == StripInput::Toggle {
            let backend = self.media_backend.clone().or_else(|| {
                media_preview::sandbox::Sandbox::current()
                    .ok()
                    .map(|b| Rc::new(b) as Rc<dyn media_preview::engine::Backend>)
            });
            let Some(backend) = backend else { return };
            let entity = cx.new(|cx| MediaPlayer::new(backend, None, cx));
            let subject = media_preview::player::Subject {
                path: frame.subject.path().into(),
                video: frame.kind == Kind::Video,
                box_w: (self.width * window.scale_factor()).ceil().max(1.) as u32,
                box_h: (self.height * window.scale_factor()).ceil().max(1.) as u32,
                autoplay: true,
            };
            frame._watch = Some(cx.observe(&entity, |_, _, cx| cx.notify()));
            entity.update(cx, |player, cx| {
                player.rewind_on_end = true;
                player.open(subject, cx);
            });
            *player = Some(entity);
        } else if let Some(player) = player {
            player.update(cx, |player, cx| player.strip_input(input, window, cx));
        }
        cx.notify();
    }
}
