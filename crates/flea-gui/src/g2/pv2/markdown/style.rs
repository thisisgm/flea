//! The Markdown view's theme roles and faces: each ink a laid run names, and the font each face of the document draws in.
use flea::ui_logic_pv2::markdown::markdown_flow::{Face, Ink};
use gpui::{font, Font, Hsla, Rgba};

/// The theme roles a document draws with, from the host's Omarchy theme.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    /// A fence's, a failed figure's and a code chip's ground: the window ground in Quick Look, the surface in the column.
    pub code: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub bright: Hsla,
    pub muted: Hsla,
    /// The figure helper's muted paint, separate from Source's metadata text.
    pub figure_muted: Hsla,
    pub muted_frame: Hsla,
    /// The accent lifted to 3:1 against the ground, which a diagram's arrowheads and accent paints take.
    pub accent_frame: Hsla,
    /// The code reply's roles 1 to 6: red, green, yellow, blue, magenta and cyan.
    pub inks: [Hsla; 6],
}

/// A table rule's and the bar hairline's wash over the foreground.
pub const RULE_WASH: f32 = 0.12;

impl Palette {
    /// The colour a laid run's ink names; a markup colour that does not parse draws in the foreground.
    pub fn ink(&self, ink: &Ink) -> Hsla {
        match ink {
            Ink::Foreground => self.foreground,
            Ink::Bright => self.bright,
            Ink::Muted => self.muted,
            Ink::Code(role) => usize::try_from(*role)
                .ok()
                .and_then(|role| role.checked_sub(1))
                .and_then(|role| self.inks.get(role).copied())
                .unwrap_or(self.foreground),
            Ink::Hex(hex) => Rgba::try_from(hex.as_str()).map_or(self.foreground, Hsla::from),
        }
    }

    pub fn rule(&self) -> Hsla {
        self.foreground.opacity(RULE_WASH)
    }
}

/// "#rrggbb" of a theme colour, the form the parser and the figure helper take.
pub fn hex(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(rgba.r),
        byte(rgba.g),
        byte(rgba.b)
    )
}

/// The document's family and the symbol face the task boxes draw in.
#[derive(Clone, Debug, PartialEq)]
pub struct Faces {
    pub family: String,
    pub symbol: String,
}

impl Faces {
    pub fn omarchy_default() -> Self {
        Faces {
            family: "JetBrainsMono Nerd Font".into(),
            symbol: "Noto Sans Symbols 2".into(),
        }
    }

    pub fn font(&self, face: Face) -> Font {
        let base = font(self.family.clone());
        match face {
            Face::Regular => base,
            Face::Bold => base.bold(),
            Face::Italic => base.italic(),
            Face::BoldItalic => base.bold().italic(),
            Face::Symbol => font(self.symbol.clone()),
        }
    }
}
