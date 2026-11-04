use super::MARK_FILL;
use crate::{palette::Theme, row::NameInk};
use gpui::{Hsla, TextStyle};

/// The resolved theme roles shared by viewport rows and window chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub error: Hsla,
    pub error_on_surface: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub symlink: Hsla,
    pub executable: Hsla,
    pub marked: Hsla,
    pub muted_frame: Hsla,
    /// The seven tag inks a surface draws its solid tag squares in.
    pub tags: [Hsla; 7],
}

impl Palette {
    /// An inherited background keeps its own text colour and derived inks; otherwise Omarchy's theme sets every role.
    pub fn inherited(style: &TextStyle, theme: Theme) -> Self {
        let (background, foreground) = match style.background_color {
            Some(background) => (background, style.color),
            None => (theme.background, theme.foreground),
        };
        let roles = if background == theme.background && foreground == theme.foreground {
            theme.roles()
        } else {
            crate::palette::Roles::for_inherited(background, foreground, theme.urgent)
        };
        Self {
            background,
            foreground,
            muted: roles.muted,
            error: roles.error,
            error_on_surface: roles.error_on_surface,
            metadata: roles.metadata,
            metadata_frame: roles.metadata_frame,
            accent: theme.accent,
            accent_frame: theme.accent_frame,
            symlink: roles.symlink,
            executable: roles.executable,
            marked: foreground.opacity(MARK_FILL),
            muted_frame: roles.muted_frame,
            tags: theme.tag_menu_inks,
        }
    }

    /// Glyph roles use the glass contrast recipe; callers retain the original palette for fills.
    pub(crate) fn text_inks(mut self, window: &gpui::Window, cx: &gpui::App) -> Self {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        if self.background != theme.background || self.foreground != theme.foreground {
            return self;
        }
        let lift = |ink| crate::window::glass::producers::text_ink(ink, window, cx);
        self.foreground = lift(self.foreground);
        self.muted = lift(self.muted);
        self.metadata = lift(self.metadata);
        self.metadata_frame = lift(self.metadata_frame);
        self.muted_frame = lift(self.muted_frame);
        self.symlink = lift(self.symlink);
        self.executable = lift(self.executable);
        self.error = lift(self.error);
        self.error_on_surface = lift(self.error_on_surface);
        self
    }

    /// v0.3.12's quiet text: a caption in effect in the foreground, every other one in Theme.color.muted.
    pub fn caption(&self, lit: bool) -> Hsla {
        if lit {
            self.foreground
        } else {
            self.muted
        }
    }

    /// ChromeButton.qml and TabBar.qml: a mark in effect in the foreground, every other one in Theme.color.mutedFrame.
    pub fn mark(&self, lit: bool) -> Hsla {
        if lit {
            self.foreground
        } else {
            self.muted_frame
        }
    }

    pub fn name(&self, ink: NameInk, lifted: bool) -> Hsla {
        match (lifted, ink) {
            (true, _) | (false, NameInk::Foreground) => self.foreground,
            (false, NameInk::Symlink) => self.symlink,
            (false, NameInk::Executable) => self.executable,
        }
    }
}
