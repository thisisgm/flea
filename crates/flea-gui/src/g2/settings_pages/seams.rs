//! Join seams for the G1 contracts Settings needs and BASE does not yet carry; each names its replacement.
use crate::list::Palette;
use flea::jsondoc::Json;
use flea::ui_logic_settings_pages::{about, Text};
use gpui::{Hsla, TextStyle};

/// The ink roles Settings draws with. Seam for G1's theme service: filled from the inherited text style until it lands.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ink {
    pub background: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub error: Hsla,
    // The boards' mixes over those roles: muted frame at 40%, foreground at 12, 8 and 18%, accent at 14 and 18%.
    pub rule: Hsla,
    pub hair: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub wash14: Hsla,
    pub wash18: Hsla,
    pub scrim: Hsla,
}

impl Ink {
    pub fn inherited(style: &TextStyle, theme: crate::palette::Theme) -> Self {
        let palette = Palette::inherited(style, theme);
        // HIG settings-view and keymap draw .card on --surface, so its red is the one lifted there.
        let (surface, error) = if palette.background == theme.background {
            (theme.surface, palette.error_on_surface)
        } else {
            (palette.background, palette.error)
        };
        Self::mixed(Self {
            background: palette.background,
            surface,
            foreground: palette.foreground,
            muted: palette.muted,
            muted_frame: palette.muted_frame,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            error,
            ..Self::default()
        })
    }

    /// Fills the mixes from the base roles, as the boards' CSS mixes them.
    pub fn mixed(base: Self) -> Self {
        Self {
            rule: base.muted_frame.opacity(0.4),
            hair: base.foreground.opacity(0.12),
            hover: base.foreground.opacity(0.08),
            selected: base.foreground.opacity(0.18),
            wash14: base.accent.opacity(0.14),
            wash18: base.accent.opacity(0.18),
            scrim: base.background.opacity(0.5),
            ..base
        }
    }
}

/// The ui.json document and its writer. Seam for the wire's atomic state writer, which the window owns.
pub trait Store {
    fn document(&self) -> &Json;
    /// Merges a patch; a refused write names itself, and the panel keeps the session's values.
    fn write(&mut self, patch: &Json) -> Result<(), Text>;
}

/// An in-memory store: objects merge leaf by leaf, anything else replaces, as the state writer merges a patch.
#[derive(Clone, Debug)]
pub struct MemoryStore {
    pub document: Json,
    pub refuse: bool,
    pub writes: usize,
}

impl MemoryStore {
    pub fn new(document: Json) -> Self {
        Self {
            document,
            refuse: false,
            writes: 0,
        }
    }
}

pub fn merge(into: &mut Json, patch: &Json) {
    match (into, patch) {
        (Json::Obj(fields), Json::Obj(changes)) => {
            for (key, value) in changes {
                match fields.iter_mut().find(|(name, _)| name == key) {
                    Some((_, held)) => merge(held, value),
                    None => fields.push((key.clone(), value.clone())),
                }
            }
        }
        (into, patch) => *into = patch.clone(),
    }
}

impl Store for MemoryStore {
    fn document(&self) -> &Json {
        &self.document
    }

    fn write(&mut self, patch: &Json) -> Result<(), Text> {
        if self.refuse {
            return Err(Text::Key("settings.error.not_saved"));
        }
        if !matches!(self.document, Json::Obj(_)) {
            self.document = Json::Obj(Vec::new());
        }
        merge(&mut self.document, patch);
        self.writes += 1;
        Ok(())
    }
}

/// Unreadable compositor rounding is square until the live service answers.
pub const COMPOSITOR_CORNER_DEFAULT: f32 = crate::compositor::CORNER_DEFAULT;

/// What Settings reads but never writes. Seam for G1's Omarchy font and compositor services and the About facts.
#[derive(Clone, Debug)]
pub struct Facts {
    /// Hyprland's decoration rounding; seam for G1's compositor service, which reads it live.
    pub corner: f32,
    pub home: String,
    pub base_size: u32,
    pub monitor_scale: f64,
    pub language: String,
    /// The focused pane's folder, which Chosen folder stores; seam for G1's pane state.
    pub pane_path: String,
    pub about: about::Facts,
}

impl Default for Facts {
    fn default() -> Self {
        Self {
            corner: COMPOSITOR_CORNER_DEFAULT,
            home: String::new(),
            base_size: 14,
            monitor_scale: f64::NAN,
            language: String::new(),
            pane_path: String::new(),
            about: about::Facts {
                version: env!("CARGO_PKG_VERSION").into(),
                ..about::Facts::default()
            },
        }
    }
}

/// What the panel hands its host. Seam for G1's dialog shell and focus ring, which open, close and route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Close,
    OpenSheet,
    /// A row whose effect is the window's: a favourite, the start folder, a web route, the updater.
    Host(String),
    /// A favourite dragged by its grip from one place in the list to another.
    MoveFavourite {
        from: usize,
        to: usize,
    },
    /// A favourite's own remove mark.
    RemoveFavourite(usize),
}
