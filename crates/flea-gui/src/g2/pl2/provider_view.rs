//! The optional absent-provider footer follows the frozen source; primary boards show the installed state.
use super::{
    dialog::Focus,
    drawing::{BoxPaint, Drawn, Rect, HEIGHT},
    view::NetworkView,
};
use gpui::{px, TextRun, Window};

impl NetworkView {
    pub(super) fn footer_height(&self) -> f32 {
        if self.model.dropbox_installed {
            0.
        } else {
            79.
        }
    }
    pub(super) fn frame_height(&self, height: f32) -> f32 {
        (HEIGHT + self.footer_height() + self.status_growth()).min((height - 16.).max(0.))
    }
    pub(super) fn body_height(&self, height: f32) -> f32 {
        (self.frame_height(height) - self.footer_height()).max(0.)
    }
}
impl Drawn {
    pub(super) fn provider_footer(
        &mut self,
        view: &NetworkView,
        top: f32,
        width: f32,
        window: &Window,
    ) {
        if view.model.dropbox_installed {
            return;
        }
        let p = view.palette;
        self.boxes.push(BoxPaint {
            field: None,
            rect: Rect {
                x: 17.,
                y: top + 42.,
                w: (width - 34.).max(0.),
                h: 1.,
            },
            fill: p.muted_frame.opacity(0.4),
            border: p.muted_frame.opacity(0.),
            border_width: 0.,
            radius: 0.,
            clip: None,
        });
        self.label(
            &view.words.dropbox,
            Rect {
                x: 17.,
                y: top + 55.,
                w: (width - 34.).max(0.),
                h: 14.,
            },
            p.muted,
            12.,
            false,
            window,
        );
        let text = &view.words.install;
        let run = window.text_system().layout_line(
            text,
            px(14.),
            &[TextRun {
                len: text.len(),
                font: window.text_style().font(),
                color: p.foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        self.button(
            Focus::Dropbox,
            Rect {
                x: 17.,
                y: top + 79.,
                w: f32::from(run.width) + 20.,
                h: 30.,
            },
            text,
            view,
            window,
        );
    }
}
