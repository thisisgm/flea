//! Static key bindings shared by native frontends and their presentation surfaces.
pub mod actions;
pub mod display;
#[path = "keys_generated.rs"]
mod generated;
pub mod presentation;
pub mod sequence;
pub mod user;
pub use generated::*;

#[cfg(test)]
#[path = "keys_document_tests.rs"]
mod document_tests;

pub const PRESETS: [&str; 4] = ["default", "vim", "mac", "windows"];
pub const CONTROL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const SUPER: u8 = 8;
pub const MODIFIER_MASK: u8 = CONTROL | SHIFT | ALT | SUPER;
/// A numeric keypad key; outside `MODIFIER_MASK`, so rows ignore it and only the digit rule sees it.
pub const KEYPAD: u8 = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Context {
    #[default]
    Listing,
    Rail,
    Menu,
    Panel,
    Preview,
    Pdf,
    Media,
    Editor,
}

impl Context {
    pub const ALL: [Self; 8] = [
        Self::Listing,
        Self::Rail,
        Self::Menu,
        Self::Panel,
        Self::Preview,
        Self::Pdf,
        Self::Media,
        Self::Editor,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Listing => "listing",
            Self::Rail => "rail",
            Self::Menu => "menu",
            Self::Panel => "panel",
            Self::Preview => "preview",
            Self::Pdf => "pdf",
            Self::Media => "media",
            Self::Editor => "editor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frontend {
    Gui,
    Tui,
}

impl Frontend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gui => "gui",
            Self::Tui => "tui",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input<'a> {
    pub key: &'a str,
    pub text: &'a str,
    pub modifiers: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    Unbound,
    Suppressed,
    Action(&'static str),
}

impl Match {
    pub fn action(self) -> &'static str {
        match self {
            Self::Action(action) => action,
            _ => "",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub preset: &'static str,
    pub mods: &'static str,
    pub key: &'static str,
    pub keys: &'static str,
    pub action: &'static str,
    pub context: &'static str,
    pub frontend: &'static str,
    pub mask: u8,
    pub context_mask: u8,
    pub label_key: &'static str,
    pub sheet: bool,
}

impl Binding {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        preset: &'static str,
        mods: &'static str,
        key: &'static str,
        keys: &'static str,
        action: &'static str,
        context: &'static str,
        frontend: &'static str,
        mask: u8,
        context_mask: u8,
        label_key: &'static str,
        sheet: bool,
    ) -> Self {
        Self {
            preset,
            mods,
            key,
            keys,
            action,
            context,
            frontend,
            mask,
            context_mask,
            label_key,
            sheet,
        }
    }

    pub fn applies(&self, context: Context, frontend: Frontend) -> bool {
        (self.context_mask & (1 << context as u8) != 0)
            && (self.frontend == "all" || self.frontend == frontend.name())
    }

    pub fn matches(&self, input: Input<'_>) -> bool {
        let mask = input.modifiers & MODIFIER_MASK;
        if self.mods == "text" {
            mask & !SHIFT == 0 && input.text == self.key
        } else {
            mask == self.mask && input.key.eq_ignore_ascii_case(self.key)
        }
    }

    pub fn input(&self) -> Input<'static> {
        Input {
            key: if self.mods == "text" { "" } else { self.key },
            text: if self.mods == "text" { self.key } else { "" },
            modifiers: self.mask,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Digit {
    pub text: &'static str,
    pub action: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct SheetBinding {
    pub action: &'static str,
    pub keys: &'static str,
    pub label_key: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct SheetGroup {
    pub id: &'static str,
    pub label_key: &'static str,
    pub actions: &'static [&'static str],
}

#[derive(Clone, Copy, Debug)]
pub struct SequenceBinding {
    pub arm: &'static str,
    pub action: &'static str,
    pub paired: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PointerBinding {
    pub place: &'static str,
    pub press: &'static str,
    pub row: &'static str,
    pub effect: &'static str,
    pub label_key: &'static str,
    pub button: &'static str,
    pub mask: u8,
    pub taps: u8,
}

impl PointerBinding {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        place: &'static str,
        press: &'static str,
        row: &'static str,
        effect: &'static str,
        label_key: &'static str,
        button: &'static str,
        mask: u8,
        taps: u8,
    ) -> Self {
        Self {
            place,
            press,
            row,
            effect,
            label_key,
            button,
            mask,
            taps,
        }
    }
}

pub fn preset(name: &str) -> &'static str {
    PRESETS
        .into_iter()
        .find(|known| *known == name)
        .unwrap_or(PRESETS[0])
}

pub fn lookup(name: &str, context: Context, frontend: Frontend, input: Input<'_>) -> Match {
    for pass in [preset(name), "all"] {
        if let Some(row) = PRESET_KEYS
            .iter()
            .find(|row| row.preset == pass && row.applies(context, frontend) && row.matches(input))
        {
            return if row.action.is_empty() {
                Match::Suppressed
            } else {
                Match::Action(row.action)
            };
        }
    }
    if !matches!(context, Context::Listing | Context::Rail) {
        return Match::Unbound;
    }
    if let Some(row) = SHARED_KEYS.iter().find(|row| row.matches(input)) {
        return Match::Action(row.action);
    }
    if input.modifiers == 0 {
        if let Some(digit) = DIGITS.iter().find(|digit| digit.text == input.text) {
            return Match::Action(digit.action);
        }
    }
    Match::Unbound
}

/// Looks up `first`, then `second` when `first` is unbound: a key's two paths in the keyboard policy's order.
pub fn lookup_either(
    name: &str,
    context: Context,
    frontend: Frontend,
    first: Input<'_>,
    second: Option<Input<'_>>,
) -> Match {
    match (lookup(name, context, frontend, first), second) {
        (Match::Unbound, Some(second)) => lookup(name, context, frontend, second),
        (found, _) => found,
    }
}

pub fn binding_rows(name: &str, frontend: Frontend) -> impl Iterator<Item = &'static Binding> {
    let name = preset(name);
    PRESET_KEYS.iter().chain(SHARED_KEYS).filter(move |row| {
        (row.preset == "all" || row.preset == name)
            && !row.action.is_empty()
            && row.applies(Context::Listing, frontend)
            && lookup(name, Context::Listing, frontend, row.input()).action() == row.action
    })
}

pub fn action_group(action: &str) -> &str {
    SEQUENCES
        .iter()
        .find(|row| row.arm == action)
        .map_or(action, |row| row.action)
}

pub fn pointer(
    place: &str,
    row: &str,
    button: &str,
    modifiers: u8,
    taps: u8,
) -> Option<&'static PointerBinding> {
    let applies = |binding: &&PointerBinding| {
        binding.place == place
            && binding.row == row
            && binding.button == button
            && binding.mask == modifiers & MODIFIER_MASK
            && binding.taps == taps
    };
    POINTER.iter().find(applies).or_else(|| {
        POINTER.iter().find(|binding| {
            binding.place == place
                && binding.row == "any"
                && binding.button == button
                && binding.mask == modifiers & MODIFIER_MASK
                && binding.taps == taps
        })
    })
}
