//! The user's own key file: chords bound to built-in or plugin action ids, in keys.toml's table shape.
use super::actions::Id;
use super::{Input, PRESET_KEYS, SHARED_KEYS};

/// Where the file lives under the config home (`$XDG_CONFIG_HOME`, else `~/.config`).
pub const FILE: &str = "flea/keys.toml";
pub const MAX_BYTES: usize = 64 * 1024;
pub const MAX_BINDINGS: usize = 256;
// Chords the window reads before the key table, so a binding there could never fire.
const RESERVED: [(u8, &str); 2] = [(super::CONTROL, "Q"), (super::CONTROL, "P")];
const MODS: [(&str, u8); 9] = [
    ("none", 0),
    ("text", 0),
    ("ctrl", super::CONTROL),
    ("shift", super::SHIFT),
    ("ctrlshift", super::CONTROL | super::SHIFT),
    ("alt", super::ALT),
    ("super", super::SUPER),
    ("supershift", super::SUPER | super::SHIFT),
    ("superalt", super::SUPER | super::ALT),
];
const SHARED: [(&str, &str); 5] = [
    ("ctrlshift", "ctrlshift"),
    ("ctrl", "ctrl"),
    ("shift", "shift"),
    ("code", "none"),
    ("text", "text"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserBinding {
    /// A preset name, or "all".
    pub preset: String,
    pub mods: &'static str,
    pub mask: u8,
    pub key: String,
    pub action: String,
    /// The 1-based line of the block's header, for messages.
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserKeys {
    pub bindings: Vec<UserBinding>,
    pub problems: Vec<Problem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The binding is not for this preset, or names a plugin action no enabled plugin declares.
    Inactive,
    Live,
    /// The built-in action that already answers this chord in the preset, or "" for a chord the window reserves.
    Taken(&'static str),
    /// An earlier live binding holds the same chord.
    Repeated,
    /// No built-in action has this id.
    Unknown,
    /// The active layout cannot supply the raw state needed to verify this binding.
    Unavailable,
}

impl UserBinding {
    pub fn is_plugin(&self) -> bool {
        is_plugin_id(&self.action)
    }

    pub fn input(&self) -> Input<'_> {
        if self.mods == "text" {
            Input {
                key: "",
                text: &self.key,
                modifiers: 0,
            }
        } else {
            Input {
                key: &self.key,
                text: "",
                modifiers: self.mask,
            }
        }
    }

    /// The same rule a generated row matches with: text rows take their character with or without Shift.
    pub fn matches(&self, input: Input<'_>) -> bool {
        let mask = input.modifiers & super::MODIFIER_MASK;
        if self.mods == "text" {
            mask & !super::SHIFT == 0 && input.text == self.key
        } else {
            mask == self.mask && input.key.eq_ignore_ascii_case(&self.key)
        }
    }

    /// The chord spelled the way keys.toml's `keys` column spells it: ctrl-shift-k, f5, K.
    pub fn keys(&self) -> String {
        if self.mods == "text" {
            return self.key.clone();
        }
        let prefix = match self.mods {
            "ctrl" => "ctrl-",
            "shift" => "shift-",
            "ctrlshift" => "ctrl-shift-",
            "alt" => "alt-",
            "super" => "super-",
            "supershift" => "super-shift-",
            "superalt" => "super-alt-",
            _ => "",
        };
        let key = if self.key.eq_ignore_ascii_case("return") {
            "enter".to_owned()
        } else {
            self.key.to_ascii_lowercase()
        };
        format!("{prefix}{key}")
    }

    fn applies(&self, preset: &str) -> bool {
        self.preset == "all" || self.preset == preset
    }

    pub fn required_modifiers(&self) -> [u8; 2] {
        if self.mods == "text" {
            [0, super::SHIFT]
        } else {
            [self.mask; 2]
        }
    }
}

/// `plugin:action`, the leaf form a plugin menu row dispatches, as `flea --action` reads it.
pub fn is_plugin_id(id: &str) -> bool {
    matches!(super::actions::parse(id), Some(Id::Plugin { .. }))
}

/// The key table's own `&'static str` for a built-in action id, through the one resolver `flea --action` uses.
pub fn builtin(id: &str) -> Option<&'static str> {
    match super::actions::parse(id) {
        Some(Id::Builtin(action)) => Some(action),
        _ => None,
    }
}

/// The label catalog key the key table gives a built-in action.
pub fn builtin_label(id: &str) -> Option<&'static str> {
    PRESET_KEYS
        .iter()
        .chain(SHARED_KEYS)
        .find(|row| row.action == id && !row.label_key.is_empty())
        .map(|row| row.label_key)
}

impl UserKeys {
    pub fn has_plugin_bindings(&self) -> bool {
        self.bindings.iter().any(UserBinding::is_plugin)
    }
}

#[path = "user_projection.rs"]
mod projection;
pub use projection::{Projection, Sample};

#[path = "user_parse.rs"]
mod parse;
pub use parse::parse;

#[cfg(test)]
#[path = "user_test_projection.rs"]
mod test_projection;

#[cfg(test)]
#[path = "user_tests.rs"]
mod tests;
