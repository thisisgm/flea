//! pv1's chrome marks: Lucide 1.38.0 path data (ISC) as ui/js/Icons.js carries it, beside the row marks it reuses.
use crate::row::marks::Mark;
use gpui::{Bounds, Hsla, Pixels, SharedString, TransformationMatrix, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Row(Mark),
    Cube,
    Markdown,
    Alert,
    Close,
    Minus,
    Plus,
    Maximize,
    ChevronLeft,
    ChevronRight,
    Info,
    Filmstrip,
    Lock,
}

macro_rules! svg {
    ($d:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
            r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
            $d,
            r#""/></svg>"#
        )
        .as_bytes()
    };
}

impl Glyph {
    /// The catalog key of a control's accessible name, as ChromeButton's accessName gives it.
    pub fn access_key(self) -> Option<&'static str> {
        match self {
            Self::Close => Some("preview.close"),
            Self::Minus => Some("preview.zoom_out"),
            Self::Plus => Some("preview.zoom_in"),
            Self::Maximize => Some("preview.expand"),
            _ => None,
        }
    }

    /// A strip control's glyph by the chrome name a view gives it; None for a name pv1 does not draw.
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "chevron-left" => Self::ChevronLeft,
            "chevron-right" => Self::ChevronRight,
            "minus" => Self::Minus,
            "plus" => Self::Plus,
            "maximize" => Self::Maximize,
            "x" => Self::Close,
            _ => return None,
        })
    }

    /// Icons.js glyphFor(): a kind's mark by its icon name, the file mark when none is known.
    pub fn kind(icon: &str, mode: u32) -> Self {
        Self::Row(Mark::for_row(icon, mode))
    }

    pub fn svg(self) -> (SharedString, &'static [u8]) {
        let (key, data) = match self {
            Self::Row(mark) => return mark.svg(),
            Self::Cube => (
                "pv1-mark:cube",
                svg!("M12 2l9 5v10l-9 5-9-5V7z M3 7l9 5 9-5 M12 12v10"),
            ),
            Self::Markdown => (
                "pv1-mark:markdown",
                svg!("M2 5h20v14H2z M5.5 15V9l3 3.5 3-3.5v6 M17 9v6 M14.5 12.5 17 15l2.5-2.5"),
            ),
            Self::Alert => (
                "pv1-mark:alert",
                svg!("M12 2 23 21H1z M12 10v4 M12 18L12.01 18"),
            ),
            Self::Close => ("pv1-mark:x", svg!("M6 6l12 12 M18 6 6 18")),
            Self::Minus => ("pv1-mark:minus", svg!("M5 12h14")),
            Self::Plus => ("pv1-mark:plus", svg!("M5 12h14 M12 5v14")),
            Self::Maximize => (
                "pv1-mark:maximize",
                svg!("M8 3H3v5 M16 3h5v5 M8 21H3v-5 M16 21h5v-5"),
            ),
            Self::ChevronLeft => ("pv1-mark:chevron-left", svg!("m15 18-6-6 6-6")),
            Self::ChevronRight => ("pv1-mark:chevron-right", svg!("m9 18 6-6-6-6")),
            Self::Info => (
                "pv1-mark:info",
                svg!("M12 16v-4 M12 8h.01 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0z"),
            ),
            Self::Filmstrip => (
                "pv1-mark:filmstrip",
                svg!("M4 4h16v16H4z M4 8h16 M4 16h16 M8 4v4 M16 4v4 M8 16v4 M16 16v4"),
            ),
            Self::Lock => (
                "pv1-mark:lock",
                svg!("M5 11h14v10H5z M8 11V7a4 4 0 0 1 8 0v4 M12 16L12.01 16"),
            ),
        };
        (SharedString::new_static(key), data)
    }

    pub fn paint(self, bounds: Bounds<Pixels>, ink: Hsla, window: &mut Window, cx: &mut gpui::App) {
        let (key, data) = self.svg();
        let _ = window.paint_svg(
            bounds,
            key,
            Some(data),
            TransformationMatrix::unit(),
            ink,
            cx,
        );
    }
}
