//! The Markdown view's theme roles, faces and tokens: each ink a laid run names, the font each face of the document draws in, and the scaled sizes it lays out with.
use flea::ui_logic_pv2::markdown::{
    markdown_flow::{Face, Ink},
    markdown_layout::Metrics,
};
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
    /// The figure helper's muted paint, from the window theme rather than the inherited ground.
    pub figure_muted: Hsla,
    pub muted_frame: Hsla,
    /// The accent lifted to 3:1 against the ground, which a diagram's arrowheads and accent paints take.
    pub accent_frame: Hsla,
    /// The code reply's roles 1 to 6: red, green, yellow, blue, magenta and cyan.
    pub inks: [Hsla; 6],
}

/// The host's scaled theme tokens a document lays out with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// The document's body: the theme's body in Quick Look, its small body in the column.
    pub body: f32,
    pub theme_body: f32,
    pub caption: f32,
    pub gap: f32,
    pub hairline: f32,
    pub row_pad_x: f32,
    pub row_pad_y: f32,
    /// The chrome mark's size.
    pub mark: f32,
    /// The state mark's size, which a refused Source draws.
    pub state_mark: f32,
    /// Source's row where a board fixes it, the code view's own (ql-markdown-source: 20), never under the face's extent; None takes the body's line box, never shorter than the face's extent.
    pub source_line: Option<f32>,
    /// True for the preview column's document; Quick Look's is false.
    pub column: bool,
}

impl Tokens {
    pub fn metrics(&self) -> Metrics {
        Metrics {
            body: self.body,
            theme_body: self.theme_body,
            caption: self.caption,
            gap: self.gap,
            hairline: self.hairline,
            row_pad_x: self.row_pad_x,
            mark: self.mark,
            column: self.column,
        }
    }

    /// The document's inset across: the row padding and the control padding, less the frame's hairline.
    pub fn inset_x(&self) -> f32 {
        self.row_pad_x + self.row_pad_y - self.hairline
    }

    pub fn inset_y(&self) -> f32 {
        self.metrics().inset_y()
    }

    /// Source retains its existing line viewport and padding.
    pub fn source_inset_y(&self) -> f32 {
        self.gap + self.row_pad_y
    }
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
