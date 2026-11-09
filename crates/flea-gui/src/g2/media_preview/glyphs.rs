//! Strip and state glyphs: Lucide 1.38.0 path data (ISC), the Omarchy cut the boards draw, as alpha masks.
use gpui::SharedString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Play,
    Pause,
    Volume,
    VolumeX,
    Music,
    Alert,
}

macro_rules! svg {
    ($($d:literal),+) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
            r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter">"#,
            $(r#"<path d=""#, $d, r#""/>"#,)+
            "</svg>"
        )
        .as_bytes()
    };
}

impl Glyph {
    pub fn svg(self) -> (SharedString, &'static [u8]) {
        let (key, data): (&'static str, &'static [u8]) = match self {
            Self::Play => ("media/play", svg!("M6 4l14 8-14 8z")),
            Self::Pause => ("media/pause", svg!("M14 3h5v18h-5z", "M5 3h5v18H5z")),
            Self::Volume => (
                "media/volume",
                svg!(
                    "M11 5L6 9H2v6h4l5 4z",
                    "M15.5 8.5a5 5 0 0 1 0 7",
                    "M19 5a10 10 0 0 1 0 14"
                ),
            ),
            Self::VolumeX => (
                "media/volume-x",
                svg!("M11 5L6 9H2v6h4l5 4z", "M22 9l-6 6", "M16 9l6 6"),
            ),
            Self::Music => (
                "media/music",
                svg!("M9 18V5l12-2v13", "M3 15h6v6H3z", "M15 13h6v6h-6z"),
            ),
            Self::Alert => (
                "media/alert",
                svg!("M12 2 23 21H1z", "M12 10v4", "M12 18L12.01 18"),
            ),
        };
        (SharedString::new_static(key), data)
    }
}
