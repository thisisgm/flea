//! The unfocused-window scene's live counters; a driver samples them twice and compares.
use crate::state::Shell;
use flea::jsondoc::Json;
use gpui::{Context, PresentedFrames, Subscription, Window};

#[derive(Default)]
pub(crate) struct Activity {
    /// Root renders, one per window draw: the scene's frame-request count.
    pub(crate) draws: u64,
    notifications: u64,
    pub(super) notified: Option<Subscription>,
}

impl Activity {
    pub(super) fn follow(&mut self, cx: &mut Context<Shell>) {
        self.notified =
            Some(cx.observe_self(|shell, _| shell.presentation.activity.notifications += 1));
    }

    pub(crate) fn json(&self, window: &Window, frames: Option<PresentedFrames>) -> Json {
        use crate::visual::{number, object};
        let frame = |count: fn(&PresentedFrames) -> u64| {
            frames.as_ref().map_or(Json::Null, |f| number(count(f)))
        };
        object([
            ("active", Json::Bool(window.is_window_active())),
            ("draws", number(self.draws)),
            ("notifications", number(self.notifications)),
            ("committed", frame(|f| f.committed)),
            ("presented", frame(|f| f.presented)),
        ])
    }
}
