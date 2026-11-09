//! What the Plugins section needs from units joined later: the backend line, the theme roles and the Settings host.
use gpui::Hsla;

/// Sends one plugins request line to the backend; replies come back through PluginsPane::receive.
pub trait PluginsWire {
    fn send(&self, line: String) -> Result<(), String>;
}

/// The Settings rail row this section registers with st1's host: its catalog key and its plug mark, cut in the Omarchy edge.
pub const SECTION: (&str, &str) = ("plugins.title", "plug");

/// What a key or click asks of the Settings host rather than of the section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Esc on the installed list: the host closes Settings as for any section.
    Close,
    /// A pinned shelf entry: the host opens it.
    Open(String),
    /// The Pinned heading's Pin This Folder: the host pins the focused pane's folder.
    PinFolder,
    /// AirDrop's Receiving: on turns the radio on for ten minutes, receiving into the folder named.
    AirdropReceiving(bool, String),
    /// AirDrop's Save received files to: the host opens Flea's folder chooser at the folder named.
    AirdropFolder(String),
}

/// The palette roles the boards name; filled from Omarchy's theme by the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub bg: Hsla,
    pub surface: Hsla,
    pub fg: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub error: Hsla,
}

impl Tokens {
    /// Reads the shared palette roles and retains the inherited surface ground.
    pub fn interim(palette: &crate::list::Palette) -> Self {
        Self {
            bg: palette.background,
            surface: palette.background,
            fg: palette.foreground,
            muted: palette.muted,
            muted_frame: palette.muted_frame,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error: palette.error,
        }
    }

    /// HIG plugin-add and plugin-git-page draw the pane inside the Settings .card, on --surface.
    pub fn on_surface(palette: &crate::list::Palette, theme: crate::palette::Theme) -> Self {
        Self {
            surface: theme.surface,
            error: palette.error_on_surface,
            ..Self::interim(palette)
        }
    }

    /// The boards' --frame.
    pub fn frame(&self) -> Hsla {
        self.muted_frame
    }

    /// The boards' --rule, muted frame at 40%.
    pub fn rule(&self) -> Hsla {
        self.muted_frame.opacity(0.4)
    }

    /// The boards' --hover, foreground at 8%, the pane cursor and the hover lift.
    pub fn hover(&self) -> Hsla {
        self.fg.opacity(0.08)
    }

    /// The boards' --wash14, accent at 14%, behind the default button.
    pub fn wash14(&self) -> Hsla {
        self.accent.opacity(0.14)
    }

    /// The boards' --track, foreground at 6%.
    pub fn track(&self) -> Hsla {
        self.fg.opacity(0.06)
    }
}

#[cfg(test)]
impl super::view::PluginsPane {
    pub(crate) fn tokens(&self) -> Tokens {
        self.tokens
    }
}
