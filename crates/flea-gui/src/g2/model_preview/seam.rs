//! What the 3D view needs from outside: a line to the backend, a model reader and the host's state and theme.
use flea::ui_logic_model_preview::{color::Palette, scene::Scene, wire::Data};
use gpui::{Hsla, Rgba};
use std::path::Path;

/// Sends one request line to the backend; false when the backend is gone.
pub trait Wire {
    fn send(&self, line: String) -> bool;
}

/// Reads the backend's cached model file into a scene, off the main thread.
pub trait ModelReader: Send + Sync {
    fn read(&self, file: &Path, data: &Data) -> Result<Scene, String>;
}

/// The stand-in reader until the backend's model file format joins: every model is refused, so the view says it cannot preview.
pub struct NoReader;

impl ModelReader for NoReader {
    fn read(&self, _: &Path, _: &Data) -> Result<Scene, String> {
        Err("no model reader".into())
    }
}

/// The host's state for one mounted view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Host {
    /// The window has keyboard focus.
    pub focused: bool,
    /// The preview is on screen.
    pub shown: bool,
    /// Hyprland's animations are on.
    pub animations: bool,
    /// The card or column ground the view sits on.
    pub ground: Hsla,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            focused: true,
            shown: true,
            animations: true,
            ground: crate::palette::Theme::default().background,
        }
    }
}

fn rgb(color: Hsla) -> [f32; 3] {
    let Rgba { r, g, b, .. } = color.to_rgb();
    [r, g, b]
}

/// The view's palette from the window's roles: the foreground's two shades, the muted hairlines and the frame edge.
pub fn palette(roles: &crate::list::Palette, ground: Hsla) -> Palette {
    let frame = crate::presentation::frames::Frames::from_palette(roles, 0.).ink;
    Palette {
        ground: rgb(ground),
        foreground: rgb(roles.foreground),
        muted: rgb(roles.metadata),
        frame: rgb(frame),
    }
}

/// The production reader consumes the bounded sandbox cache, off the UI thread.
pub struct CacheReader;
impl ModelReader for CacheReader {
    fn read(&self, file: &Path, _: &Data) -> Result<Scene, String> {
        flea::preview_model::read(file)
    }
}

#[cfg(test)]
mod tests {
    /// The fixture tells muted_frame from metadata_frame, so an edge drawn from another role fails.
    #[test]
    fn the_frame_edge_takes_the_shared_frame_ink() {
        let theme = crate::presentation::frame_gate::theme();
        let roles = crate::list::Palette::inherited(&gpui::TextStyle::default(), theme);
        let frames = crate::presentation::frames::Frames::from_palette(&roles, 0.);
        let ink = super::rgb(frames.ink);
        assert_ne!(ink, super::rgb(roles.metadata_frame));
        assert_eq!(super::palette(&roles, theme.background).frame, ink);
    }
}
