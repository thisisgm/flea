//! The card's observation-only readers: keyboard focus and the scrolled body height.
use super::view::NetworkView;
use gpui::Window;

impl NetworkView {
    /// Observation only: whether the window's keyboard focus sits inside the card.
    pub fn holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focus.contains_focused(window, cx)
    }
    /// Observation only: the height the body scrolls over, title and buttons included.
    pub fn scroll_content(&self) -> f32 {
        self.content_height()
    }
}
