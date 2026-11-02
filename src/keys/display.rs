//! Immutable key labels resolved before input dispatch and paint.
use std::fmt;

#[cfg(test)]
#[path = "display_symbol_tests.rs"]
mod symbol_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeycapLayout {
    layout: &'static str,
    variant: &'static str,
    name: &'static str,
    overrides: &'static [(&'static str, &'static str)],
    ambiguous: &'static [&'static str],
    diagnostics: &'static [&'static str],
    unavailable: &'static [&'static str],
}

/// A resolved active keyboard group, independent of catalog language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeycapSnapshot {
    layout: &'static KeycapLayout,
    group: u32,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeycapLabel {
    pub text: String,
    pub canonical: String,
    pub layout_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeycapError {
    UnknownLayout { layout: String, variant: String },
    UnknownGroupName(String),
    InvalidGroup(u32),
    UnknownChord(String),
    AmbiguousChord(String),
    UnavailableChord(String),
    InvalidChord(String),
}

impl fmt::Display for KeycapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLayout { layout, variant } => {
                write!(f, "Unknown keyboard layout {layout:?}, variant {variant:?}")
            }
            Self::UnknownGroupName(name) => write!(f, "Unknown keyboard group {name:?}"),
            Self::InvalidGroup(group) => write!(f, "Invalid keyboard group {group}"),
            Self::UnknownChord(chord) => write!(f, "Unknown canonical key chord {chord:?}"),
            Self::UnavailableChord(chord) => write!(f, "Keyboard group has no path for {chord:?}"),
            Self::AmbiguousChord(chord) => {
                write!(f, "Keyboard policy gives conflicting keycaps for {chord:?}")
            }
            Self::InvalidChord(chord) => write!(f, "Invalid canonical key chord {chord:?}"),
        }
    }
}
impl std::error::Error for KeycapError {}

impl KeycapSnapshot {
    /// Resolve explicit XKB codes, optionally a comma-separated configuration and active group.
    /// A single layout code describes the active group; its index is retained as platform metadata.
    pub fn new(
        layout: &str,
        variant: Option<&str>,
        group: Option<u32>,
        generation: u64,
    ) -> Result<Self, KeycapError> {
        let group = group.unwrap_or(0);
        if group >= 4 {
            return Err(KeycapError::InvalidGroup(group));
        }
        let active = if layout.contains(',') {
            layout
                .split(',')
                .nth(group as usize)
                .ok_or(KeycapError::InvalidGroup(group))?
        } else {
            layout
        };
        let variant = variant.unwrap_or("");
        let variant = if layout.contains(',') {
            variant.split(',').nth(group as usize).unwrap_or("")
        } else if variant.contains(',') {
            variant
                .split(',')
                .nth(group as usize)
                .ok_or(KeycapError::InvalidGroup(group))?
        } else {
            variant
        };
        let resolved = KEYCAP_LAYOUTS
            .iter()
            .find(|row| row.layout == active && row.variant == variant)
            .ok_or_else(|| KeycapError::UnknownLayout {
                layout: active.to_owned(),
                variant: variant.to_owned(),
            })?;
        Ok(Self {
            layout: resolved,
            group,
            generation,
        })
    }

    /// Resolve I50-10's compiled XKB group name; never pass a translated UI language name.
    pub fn from_group_name(name: &str, group: u32, generation: u64) -> Result<Self, KeycapError> {
        if group >= 4 {
            return Err(KeycapError::InvalidGroup(group));
        }
        let layout = KEYCAP_LAYOUTS
            .iter()
            .find(|row| row.name == name)
            .ok_or_else(|| KeycapError::UnknownGroupName(name.to_owned()))?;
        Ok(Self {
            layout,
            group,
            generation,
        })
    }

    pub fn layout(&self) -> &'static str {
        self.layout.layout
    }
    pub fn variant(&self) -> &'static str {
        self.layout.variant
    }
    pub fn group(&self) -> u32 {
        self.group
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Canonical spellings with unresolved reachability or collisions in the source matrix.
    /// Display labels do not prove that a frontend dispatches these commands correctly.
    pub fn diagnostics(&self) -> &'static [&'static str] {
        self.layout.diagnostics
    }

    /// Borrow an already resolved atom or canonical sequence without allocation.
    pub fn label(&self, canonical: &str) -> Result<&'static str, KeycapError> {
        if self.layout.unavailable.contains(&canonical) {
            return Err(KeycapError::UnavailableChord(canonical.to_owned()));
        }
        if self.layout.ambiguous.contains(&canonical) {
            return Err(KeycapError::AmbiguousChord(canonical.to_owned()));
        }
        self.layout
            .overrides
            .iter()
            .chain(KEYCAP_DEFAULTS)
            .find(|row| row.0 == canonical)
            .map(|row| row.1)
            .ok_or_else(|| KeycapError::UnknownChord(canonical.to_owned()))
    }

    /// Canonical spellings whose keycap the policy cannot settle on this layout.
    pub fn ambiguous(&self) -> &'static [&'static str] {
        self.layout.ambiguous
    }

    /// Like `label`, but an ambiguous chord keeps its canonical legend; unknown and unavailable ones still fail.
    pub fn legend(&self, canonical: &str) -> Result<&'static str, KeycapError> {
        match self.label(canonical) {
            Err(KeycapError::AmbiguousChord(_)) => KEYCAP_DEFAULTS
                .iter()
                .find(|row| row.0 == canonical)
                .map(|row| row.1)
                .ok_or_else(|| KeycapError::UnknownChord(canonical.to_owned())),
            result => result,
        }
    }
}

/// Resolve alternatives separated by " / " and sequence parts separated by spaces.
/// The result is logical text; each frontend supplies its own bidi isolation when rendering.
pub fn chord_label(canonical: &str, snapshot: &KeycapSnapshot) -> Result<KeycapLabel, KeycapError> {
    compose(canonical, snapshot, KeycapSnapshot::label)
}

/// `chord_label` with each part resolved by `KeycapSnapshot::legend`, so one ambiguous part never withholds the rest.
pub fn chord_legend(
    canonical: &str,
    snapshot: &KeycapSnapshot,
) -> Result<KeycapLabel, KeycapError> {
    compose(canonical, snapshot, KeycapSnapshot::legend)
}

// Sample: "dd / ctrl-c ctrl-v", alternatives split on " / " and sequence parts on single spaces.
fn compose(
    canonical: &str,
    snapshot: &KeycapSnapshot,
    atom: fn(&KeycapSnapshot, &str) -> Result<&'static str, KeycapError>,
) -> Result<KeycapLabel, KeycapError> {
    let mut alternatives = Vec::new();
    for alternative in canonical.split(" / ") {
        let mut parts = Vec::new();
        for part in alternative.split(' ') {
            if part.is_empty() {
                return Err(KeycapError::InvalidChord(canonical.to_owned()));
            }
            parts.push(atom(snapshot, part)?);
        }
        alternatives.push(parts.join(" "));
    }
    Ok(KeycapLabel {
        text: alternatives.join(" / "),
        canonical: canonical.to_owned(),
        layout_generation: snapshot.generation,
    })
}

// BEGIN GENERATED KEYCAPS
// Generated from keys.toml by tools/flea-keymap-gen --keycaps.
#[rustfmt::skip]
static KEYCAP_DEFAULTS: &[(&str, &str)] = &[("+", "+"), (",", ","), (",A", ",A"), (",B", ",B"), (",M", ",M"), (",S", ",S"), (",a", ",a"), (",b", ",b"), (",e", ",e"), (",m", ",m"), (",n", ",n"), (",r", ",r"), (",s", ",s"), ("-", "-"), (".", "."), ("/", "/"), ("0", "0"), ("1", "1"), ("2", "2"), ("3", "3"), ("4", "4"), ("5", "5"), ("6", "6"), ("7", "7"), ("8", "8"), ("9", "9"), (":", ":"), (";", ";"), ("?", "?"), ("A", "A"), ("B", "B"), ("C", "C"), ("D", "D"), ("E", "E"), ("F", "F"), ("G", "G"), ("H", "H"), ("I", "I"), ("J", "J"), ("K", "K"), ("L", "L"), ("M", "M"), ("N", "N"), ("O", "O"), ("P", "P"), ("Q", "Q"), ("S", "S"), ("T", "T"), ("U", "U"), ("V", "V"), ("X", "X"), ("Y", "Y"), ("Z", "Z"), ("[", "["), ("]", "]"), ("_", "_"), ("a", "a"), ("alt-enter", "alt-enter"), ("alt-left", "alt-left"), ("alt-p", "alt-p"), ("alt-return", "alt-enter"), ("alt-right", "alt-right"), ("alt-up", "alt-up"), ("b", "b"), ("backspace", "backspace"), ("c", "c"), ("cC", "cC"), ("cD", "cD"), ("cc", "cc"), ("cd", "cd"), ("cf", "cf"), ("cn", "cn"), ("ctrl-0", "ctrl-0"), ("ctrl-1", "ctrl-1"), ("ctrl-2", "ctrl-2"), ("ctrl-3", "ctrl-3"), ("ctrl-4", "ctrl-4"), ("ctrl-[", "ctrl-["), ("ctrl-a", "ctrl-a"), ("ctrl-b", "ctrl-b"), ("ctrl-c", "ctrl-c"), ("ctrl-comma", "ctrl-comma"), ("ctrl-d", "ctrl-d"), ("ctrl-delete", "ctrl-delete"), ("ctrl-down", "ctrl-down"), ("ctrl-e", "ctrl-e"), ("ctrl-enter", "ctrl-enter"), ("ctrl-equal", "ctrl-equal"), ("ctrl-f", "ctrl-f"), ("ctrl-g", "ctrl-g"), ("ctrl-h", "ctrl-h"), ("ctrl-insert", "ctrl-insert"), ("ctrl-k", "ctrl-k"), ("ctrl-l", "ctrl-l"), ("ctrl-minus", "ctrl-minus"), ("ctrl-n", "ctrl-n"), ("ctrl-p", "ctrl-p"), ("ctrl-pagedown", "ctrl-pagedown"), ("ctrl-pageup", "ctrl-pageup"), ("ctrl-plus", "ctrl-plus"), ("ctrl-r", "ctrl-r"), ("ctrl-s", "ctrl-s"), ("ctrl-shift-.", "ctrl-shift-."), ("ctrl-shift-0", "ctrl-shift-0"), ("ctrl-shift-1", "ctrl-shift-1"), ("ctrl-shift-2", "ctrl-shift-2"), ("ctrl-shift-3", "ctrl-shift-3"), ("ctrl-shift-4", "ctrl-shift-4"), ("ctrl-shift-c", "ctrl-shift-c"), ("ctrl-shift-equal", "ctrl-shift-equal"), ("ctrl-shift-greater", "ctrl-shift-greater"), ("ctrl-shift-minus", "ctrl-shift-minus"), ("ctrl-shift-n", "ctrl-shift-n"), ("ctrl-shift-pagedown", "ctrl-shift-pagedown"), ("ctrl-shift-pageup", "ctrl-shift-pageup"), ("ctrl-shift-plus", "ctrl-shift-plus"), ("ctrl-shift-underscore", "ctrl-shift-underscore"), ("ctrl-shift-z", "ctrl-shift-z"), ("ctrl-space", "ctrl-space"), ("ctrl-t", "ctrl-t"), ("ctrl-tab", "ctrl-tab"), ("ctrl-u", "ctrl-u"), ("ctrl-up", "ctrl-up"), ("ctrl-v", "ctrl-v"), ("ctrl-w", "ctrl-w"), ("ctrl-x", "ctrl-x"), ("ctrl-y", "ctrl-y"), ("ctrl-z", "ctrl-z"), ("d", "d"), ("dd", "dd"), ("delete", "delete"), ("down", "down"), ("e", "e"), ("end", "end"), ("enter", "enter"), ("escape", "escape"), ("f", "f"), ("f1", "f1"), ("f2", "f2"), ("f4", "f4"), ("f5", "f5"), ("g", "g"), ("gc", "gc"), ("gd", "gd"), ("gf", "gf"), ("gg", "gg"), ("gh", "gh"), ("gspace", "g space"), ("gt", "gt"), ("h", "h"), ("home", "home"), ("i", "i"), ("j", "j"), ("k", "k"), ("l", "l"), ("left", "left"), ("m", "m"), ("mb", "mb"), ("menu", "menu"), ("mm", "mm"), ("mn", "mn"), ("mo", "mo"), ("mp", "mp"), ("ms", "ms"), ("n", "n"), ("o", "o"), ("p", "p"), ("pagedown", "pagedown"), ("pageup", "pageup"), ("pp", "pp"), ("q", "q"), ("r", "r"), ("return", "enter"), ("right", "right"), ("s", "s"), ("shift-backtab", "shift-tab"), ("shift-delete", "shift-delete"), ("shift-down", "shift-down"), ("shift-enter", "shift-enter"), ("shift-f10", "shift-f10"), ("shift-insert", "shift-insert"), ("shift-pagedown", "shift-pagedown"), ("shift-pageup", "shift-pageup"), ("shift-tab", "shift-tab"), ("shift-up", "shift-up"), ("space", "space"), ("super-[", "super-["), ("super-]", "super-]"), ("super-a", "super-a"), ("super-alt-v", "super-alt-v"), ("super-bracketleft", "super-bracketleft"), ("super-bracketright", "super-bracketright"), ("super-c", "super-c"), ("super-d", "super-d"), ("super-i", "super-i"), ("super-n", "super-n"), ("super-shift-.", "super-shift-."), ("super-shift-greater", "super-shift-greater"), ("super-shift-z", "super-shift-z"), ("super-v", "super-v"), ("super-z", "super-z"), ("superalt-v", "super-alt-v"), ("supershift-greater", "super-shift-greater"), ("supershift-period", "super-shift-period"), ("supershift-z", "super-shift-z"), ("t", "t"), ("tab", "tab"), ("tr", "tr"), ("tt", "tt"), ("u", "u"), ("up", "up"), ("v", "v"), ("w", "w"), ("x", "x"), ("y", "y"), ("yy", "yy"), ("z", "z"), ("{", "{"), ("}", "}"), ("~", "~")];
#[rustfmt::skip]
static KEYCAP_LAYOUTS: &[KeycapLayout] = &[
    KeycapLayout { layout: "us", variant: "", name: "English (US)", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "gb", variant: "", name: "English (UK)", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "us", variant: "dvorak", name: "English (Dvorak)", overrides: &[(",", "w"), (",A", "wA"), (",B", "wB"), (",M", "wM"), (",S", "wS"), (",a", "wa"), (",b", "wb"), (",e", "we"), (",m", "wm"), (",n", "wn"), (",r", "wr"), (",s", "ws"), ("/", "z"), (":", "shift-S"), ("?", "shift-Z"), ("[", "/"), ("ctrl-minus", "ctrl-[")], ambiguous: &["-", "ctrl-["], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", "/", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "us", variant: "colemak", name: "English (Colemak)", overrides: &[(":", "shift-O")], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "by", variant: "", name: "Belarusian", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "be", variant: "", name: "Belgian", overrides: &[(",", ";"), (",A", ";A"), (",B", ";B"), (",M", ";M"), (",S", ";S"), (",a", ";a"), (",b", ";b"), (",e", ";e"), (",m", ";m"), (",n", ";n"), (",r", ";r"), (",s", ";s"), (":", "shift-M"), (";", "m"), ("?", "shift-+"), ("[", "^"), ("]", "$"), ("_", "shift-°"), ("ctrl-[", "ctrl-^"), ("ctrl-minus", "ctrl-)"), ("super-[", "super-^"), ("super-]", "super-$"), ("super-bracketleft", "super-^"), ("super-bracketright", "super-$"), ("{", "shift-¨"), ("}", "shift-*"), ("~", "shift-³")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", ";", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "bg", variant: "phonetic", name: "Bulgarian (traditional phonetic)", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "hr", variant: "", name: "Croatian", overrides: &[(":", "shift-Č"), ("?", "shift-_"), ("[", "š"), ("]", "đ"), ("_", "shift-?"), ("ctrl-[", "ctrl-š"), ("ctrl-equal", "ctrl-+"), ("ctrl-minus", "ctrl-'"), ("super-[", "super-š"), ("super-]", "super-đ"), ("super-bracketleft", "super-š"), ("super-bracketright", "super-đ"), ("{", "shift-Š"), ("}", "shift-Đ")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "cz", variant: "qwerty", name: "Czech (QWERTY)", overrides: &[("/", "-"), (":", "shift-\""), ("?", "shift-_"), ("[", "ú"), ("]", ")"), ("_", "shift-%"), ("ctrl-[", "ctrl-ú"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-="), ("ctrl-plus", "ctrl-shift-ˇ"), ("ctrl-shift-greater", "ctrl-shift-:"), ("super-[", "super-ú"), ("super-]", "super-)"), ("super-bracketleft", "super-ú"), ("super-bracketright", "super-)"), ("super-shift-greater", "super-shift-:"), ("supershift-greater", "super-shift-:"), ("{", "shift-/"), ("}", "shift-("), ("~", "shift-˚")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "dk", variant: "", name: "Danish", overrides: &[(":", "shift-Æ"), ("?", "shift-_"), ("[", "å"), ("]", "¨"), ("_", "shift-?"), ("ctrl-[", "ctrl-å"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-+"), ("ctrl-plus", "ctrl-shift-`"), ("super-[", "super-å"), ("super-]", "super-¨"), ("super-bracketleft", "super-å"), ("super-bracketright", "super-¨"), ("{", "shift-Å"), ("}", "shift-^"), ("~", "shift-§")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "nl", variant: "", name: "Dutch", overrides: &[("/", "-"), (":", "shift-±"), ("?", "shift-="), ("ctrl-minus", "ctrl-/"), ("{", "shift-^"), ("}", "shift-|"), ("~", "shift-§")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ee", variant: "", name: "Estonian", overrides: &[(":", "shift-Ö"), ("?", "shift-_"), ("[", "ü"), ("]", "õ"), ("_", "shift-?"), ("ctrl-[", "ctrl-ü"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-+"), ("ctrl-plus", "ctrl-shift-`"), ("super-[", "super-ü"), ("super-]", "super-õ"), ("super-bracketleft", "super-ü"), ("super-bracketright", "super-õ"), ("{", "shift-Ü"), ("}", "shift-Õ"), ("~", "shift-~")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "fi", variant: "", name: "Finnish", overrides: &[(":", "shift-Ö"), ("?", "shift-_"), ("[", "å"), ("]", "¨"), ("_", "shift-?"), ("ctrl-[", "ctrl-å"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-+"), ("ctrl-plus", "ctrl-shift-`"), ("super-[", "super-å"), ("super-]", "super-¨"), ("super-bracketleft", "super-å"), ("super-bracketright", "super-¨"), ("{", "shift-Å"), ("}", "shift-^"), ("~", "shift-½")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "fr", variant: "", name: "French", overrides: &[(",", ";"), (",A", ";A"), (",B", ";B"), (",M", ";M"), (",S", ";S"), (",a", ";a"), (",b", ";b"), (",e", ";e"), (",m", ";m"), (",n", ";n"), (",r", ";r"), (",s", ";s"), (":", "shift-M"), (";", "m"), ("?", "shift-§"), ("[", "^"), ("]", "$"), ("_", "shift-°"), ("ctrl-[", "ctrl-^"), ("ctrl-minus", "ctrl-)"), ("ctrl-shift-underscore", "ctrl-shift-°"), ("super-[", "super-^"), ("super-]", "super-$"), ("super-bracketleft", "super-^"), ("super-bracketright", "super-$"), ("{", "shift-¨"), ("}", "shift-£")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", ";", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ca", variant: "", name: "French (Canada)", overrides: &[("?", "shift-É"), ("[", "^"), ("]", "¸"), ("ctrl-[", "ctrl-^"), ("super-[", "super-^"), ("super-]", "super-¸"), ("super-bracketleft", "super-^"), ("super-bracketright", "super-¸"), ("{", "shift-^"), ("}", "shift-¨"), ("~", "shift-|")], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ch", variant: "fr", name: "French (Switzerland)", overrides: &[(":", "shift-ö"), ("?", "shift-_"), ("[", "è"), ("]", "¨"), ("_", "shift-?"), ("ctrl-[", "ctrl-è"), ("ctrl-equal", "ctrl-^"), ("ctrl-minus", "ctrl-'"), ("ctrl-plus", "ctrl-shift-`"), ("ctrl-shift-plus", "ctrl-shift-`"), ("super-[", "super-è"), ("super-]", "super-¨"), ("super-bracketleft", "super-è"), ("super-bracketright", "super-¨"), ("{", "shift-ü"), ("}", "shift-!"), ("~", "shift-°")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ge", variant: "", name: "Georgian", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "de", variant: "", name: "German", overrides: &[(":", "shift-Ö"), ("?", "shift-_"), ("[", "ü"), ("]", "+"), ("_", "shift-?"), ("ctrl-[", "ctrl-ü"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-ß"), ("super-[", "super-ü"), ("super-]", "super-+"), ("super-bracketleft", "super-ü"), ("super-bracketright", "super-+"), ("{", "shift-Ü"), ("}", "shift-*"), ("~", "shift-°")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ch", variant: "", name: "German (Switzerland)", overrides: &[(":", "shift-é"), ("?", "shift-_"), ("[", "ü"), ("]", "¨"), ("_", "shift-?"), ("ctrl-[", "ctrl-ü"), ("ctrl-equal", "ctrl-^"), ("ctrl-minus", "ctrl-'"), ("ctrl-plus", "ctrl-shift-`"), ("ctrl-shift-plus", "ctrl-shift-`"), ("super-[", "super-ü"), ("super-]", "super-¨"), ("super-bracketleft", "super-ü"), ("super-bracketright", "super-¨"), ("{", "shift-è"), ("}", "shift-!"), ("~", "shift-°")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "gr", variant: "", name: "Greek", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "il", variant: "", name: "Hebrew", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "hu", variant: "", name: "Hungarian", overrides: &[("0", "ö"), (":", "shift-É"), (";", "é"), ("?", "shift-_"), ("[", "ő"), ("]", "ú"), ("_", "shift-Ü"), ("ctrl-0", "ctrl-ö"), ("ctrl-[", "ctrl-ő"), ("ctrl-minus", "ctrl-ü"), ("ctrl-plus", "ctrl-shift-Ó"), ("ctrl-shift-0", "ctrl-shift-Ö"), ("ctrl-shift-greater", "ctrl-shift-:"), ("ctrl-shift-plus", "ctrl-shift-Ó"), ("super-[", "super-ő"), ("super-]", "super-ú"), ("super-bracketleft", "super-ő"), ("super-bracketright", "super-ú"), ("super-shift-greater", "super-shift-:"), ("supershift-greater", "super-shift-:"), ("{", "shift-Ő"), ("}", "shift-Ú"), ("~", "shift-§")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "is", variant: "", name: "Icelandic", overrides: &[(":", "shift-Æ"), ("?", "shift-Þ"), ("[", "ð"), ("]", "'"), ("ctrl-[", "ctrl-ð"), ("ctrl-equal", "ctrl--"), ("ctrl-minus", "ctrl-ö"), ("super-[", "super-ð"), ("super-]", "super-'"), ("super-bracketleft", "super-ð"), ("super-bracketright", "super-'"), ("{", "shift-Ð"), ("}", "shift-?"), ("~", "shift-¨")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ie", variant: "", name: "Irish", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "it", variant: "", name: "Italian", overrides: &[(":", "shift-ç"), ("?", "shift-_"), ("[", "è"), ("]", "+"), ("_", "shift-?"), ("ctrl-[", "ctrl-è"), ("ctrl-equal", "ctrl-ì"), ("ctrl-minus", "ctrl-'"), ("super-[", "super-è"), ("super-]", "super-+"), ("super-bracketleft", "super-è"), ("super-bracketright", "super-+"), ("{", "shift-é"), ("}", "shift-*"), ("~", "shift-|")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "jp", variant: "", name: "Japanese", overrides: &[(":", "shift-+")], ambiguous: &["+"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus", "~"], unavailable: &[] },
    KeycapLayout { layout: "kz", variant: "", name: "Kazakh", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "kr", variant: "kr104", name: "Korean (101/104-key compatible)", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "kg", variant: "", name: "Kyrgyz", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "la", variant: "", name: "Lao", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "lv", variant: "apostrophe", name: "Latvian (apostrophe)", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "lt", variant: "", name: "Lithuanian", overrides: &[("0", "“"), ("1", "ą"), ("2", "č"), ("3", "ę"), ("4", "ė"), ("5", "į"), ("6", "š"), ("7", "ų"), ("8", "ū"), ("9", "„"), ("ctrl-0", "ctrl-“"), ("ctrl-1", "ctrl-ą"), ("ctrl-2", "ctrl-č"), ("ctrl-3", "ctrl-ę"), ("ctrl-4", "ctrl-ė"), ("ctrl-shift-0", "ctrl-shift-)"), ("ctrl-shift-1", "ctrl-shift-Ą"), ("ctrl-shift-2", "ctrl-shift-Č"), ("ctrl-shift-3", "ctrl-shift-Ę"), ("ctrl-shift-4", "ctrl-shift-Ė")], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "mk", variant: "", name: "Macedonian", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "no", variant: "", name: "Norwegian", overrides: &[(":", "shift-Ø"), ("?", "shift-_"), ("[", "å"), ("]", "¨"), ("_", "shift-?"), ("ctrl-[", "ctrl-å"), ("ctrl-equal", "ctrl-\\"), ("ctrl-minus", "ctrl-+"), ("ctrl-plus", "ctrl-shift-`"), ("super-[", "super-å"), ("super-]", "super-¨"), ("super-bracketleft", "super-å"), ("super-bracketright", "super-¨"), ("{", "shift-Å"), ("}", "shift-^"), ("~", "shift-§")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "pl", variant: "", name: "Polish", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "pt", variant: "", name: "Portuguese", overrides: &[(":", "shift-Ç"), ("?", "shift-_"), ("[", "+"), ("]", "´"), ("_", "shift-?"), ("ctrl-[", "ctrl-+"), ("ctrl-equal", "ctrl-«"), ("ctrl-minus", "ctrl-'"), ("ctrl-plus", "ctrl-shift-»"), ("super-[", "super-+"), ("super-]", "super-´"), ("super-bracketleft", "super-+"), ("super-bracketright", "super-´"), ("{", "shift-*"), ("}", "shift-`"), ("~", "shift-|")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "br", variant: "", name: "Portuguese (Brazil)", overrides: &[(":", "shift-Ç"), ("?", "shift-:"), ("~", "shift-\"")], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ro", variant: "", name: "Romanian", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ru", variant: "", name: "Russian", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "rs", variant: "latin", name: "Serbian (Latin)", overrides: &[(":", "shift-Č"), ("?", "shift-_"), ("[", "š"), ("]", "đ"), ("_", "shift-?"), ("ctrl-[", "ctrl-š"), ("ctrl-equal", "ctrl-+"), ("ctrl-minus", "ctrl-'"), ("super-[", "super-š"), ("super-]", "super-đ"), ("super-bracketleft", "super-š"), ("super-bracketright", "super-đ"), ("{", "shift-Š"), ("}", "shift-Đ")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "sk", variant: "", name: "Slovak", overrides: &[("/", "-"), (":", "shift-\""), ("?", "shift-_"), ("[", "ú"), ("]", "ä"), ("_", "shift-%"), ("ctrl-[", "ctrl-ú"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-="), ("ctrl-plus", "ctrl-shift-ˇ"), ("ctrl-shift-greater", "ctrl-shift-:"), ("super-[", "super-ú"), ("super-]", "super-ä"), ("super-bracketleft", "super-ú"), ("super-bracketright", "super-ä"), ("super-shift-greater", "super-shift-:"), ("supershift-greater", "super-shift-:"), ("{", "shift-/"), ("}", "shift-("), ("~", "shift-˚")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "si", variant: "", name: "Slovenian", overrides: &[(":", "shift-Č"), ("?", "shift-_"), ("[", "š"), ("]", "đ"), ("_", "shift-?"), ("ctrl-[", "ctrl-š"), ("ctrl-equal", "ctrl-+"), ("ctrl-minus", "ctrl-'"), ("super-[", "super-š"), ("super-]", "super-đ"), ("super-bracketleft", "super-š"), ("super-bracketright", "super-đ"), ("{", "shift-Š"), ("}", "shift-Đ"), ("~", "shift-¨")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "es", variant: "", name: "Spanish", overrides: &[(":", "shift-Ñ"), ("?", "shift-_"), ("[", "`"), ("]", "+"), ("_", "shift-?"), ("ctrl-[", "ctrl-`"), ("ctrl-equal", "ctrl-¡"), ("ctrl-minus", "ctrl-'"), ("super-[", "super-`"), ("super-]", "super-+"), ("super-bracketleft", "super-`"), ("super-bracketright", "super-+"), ("{", "shift-^"), ("}", "shift-*"), ("~", "shift-ª")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "latam", variant: "", name: "Spanish (Latin American)", overrides: &[(":", "shift-Ñ"), ("?", "shift-_"), ("_", "shift-?"), ("ctrl-equal", "ctrl-¿"), ("ctrl-minus", "ctrl-'"), ("~", "shift-°")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "se", variant: "", name: "Swedish", overrides: &[(":", "shift-Ö"), ("?", "shift-_"), ("[", "å"), ("]", "¨"), ("_", "shift-?"), ("ctrl-[", "ctrl-å"), ("ctrl-equal", "ctrl-´"), ("ctrl-minus", "ctrl-+"), ("ctrl-plus", "ctrl-shift-`"), ("super-[", "super-å"), ("super-]", "super-¨"), ("super-bracketleft", "super-å"), ("super-bracketright", "super-¨"), ("{", "shift-Å"), ("}", "shift-^"), ("~", "shift-½")], ambiguous: &["+", "-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "tj", variant: "", name: "Tajik", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "tr", variant: "", name: "Turkish", overrides: &[(",", "ö"), (",A", "öA"), (",B", "öB"), (",M", "öM"), (",S", "öS"), (",a", "öa"), (",b", "öb"), (",e", "öe"), (",m", "öm"), (",n", "ön"), (",r", "ör"), (",s", "ös"), (":", "shift-Ş"), ("?", "shift-:"), ("[", "ğ"), ("]", "ü"), ("ctrl-[", "ctrl-ğ"), ("ctrl-equal", "ctrl--"), ("ctrl-minus", "ctrl-*"), ("ctrl-plus", "ctrl-shift-_"), ("ctrl-shift-plus", "ctrl-shift-_"), ("ctrl-shift-underscore", "ctrl-shift-?"), ("super-[", "super-ğ"), ("super-]", "super-ü"), ("super-bracketleft", "super-ğ"), ("super-bracketright", "super-ü"), ("{", "shift-Ğ"), ("}", "shift-Ü"), ("~", "shift-é")], ambiguous: &["-"], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ua", variant: "", name: "Ukrainian", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
    KeycapLayout { layout: "ara", variant: "", name: "Arabic", overrides: &[], ambiguous: &[], diagnostics: &[",", ",A", ",B", ",M", ",S", ",a", ",b", ",e", ",m", ",n", ",r", ",s", "-", ":", "?", "ctrl-minus", "ctrl-shift-minus"], unavailable: &[] },
];
// END GENERATED KEYCAPS

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(layout: &str, generation: u64) -> KeycapSnapshot {
        KeycapSnapshot::new(layout, None, None, generation).unwrap()
    }
    #[test]
    fn i50_keycap_sequence_parts() {
        let s = snapshot("ru", 9);
        assert_eq!(
            chord_label("dd / ctrl-c ctrl-v", &s).unwrap().text,
            "dd / ctrl-c ctrl-v"
        );
        assert_eq!(chord_label("/", &s).unwrap().text, "/");
        assert_eq!(chord_label("ctrl-minus", &s).unwrap().text, "ctrl-minus");
    }
    #[test]
    fn i50_keycap_unknown_layout_refused() {
        assert!(KeycapSnapshot::new("not-a-layout", None, None, 0).is_err());
        assert!(KeycapSnapshot::new("us", Some("not-a-variant"), None, 0).is_err());
        assert!(KeycapSnapshot::new("us", None, Some(99), 0).is_err());
    }
    #[test]
    fn i50_keycap_omarchy_names() {
        let s = snapshot("us", 0);
        assert_eq!(
            chord_label("ctrl-shift-n / super-a / enter", &s)
                .unwrap()
                .text,
            "ctrl-shift-n / super-a / enter"
        );
        assert_eq!(chord_label("shift-backtab", &s).unwrap().text, "shift-tab");
        assert!(chord_label("ctrl-wrong-key", &s).is_err());
        assert!(chord_label("ctrl-c / ", &s).is_err());
    }
    #[test]
    fn i50_keycap_active_group_and_all_layout_collisions() {
        let named = KeycapSnapshot::from_group_name("German", 1, 7).unwrap();
        assert_eq!(named.layout(), "de");
        assert_eq!(named.group(), 1);
        assert_eq!(named.generation(), 7);
        let second = KeycapSnapshot::new("ru,us", None, Some(1), 8).unwrap();
        assert_eq!(second.layout(), "us");
        assert!(KeycapSnapshot::from_group_name("unrecorded group", 0, 9).is_err());
        let mut conflicts = 0;
        for row in KEYCAP_LAYOUTS {
            let snapshot = KeycapSnapshot::new(row.layout, Some(row.variant), None, 10).unwrap();
            assert!(!snapshot.diagnostics().is_empty());
            for (canonical, _) in KEYCAP_DEFAULTS {
                let result = chord_label(canonical, &snapshot);
                if row.unavailable.contains(canonical) {
                    assert!(matches!(result, Err(KeycapError::UnavailableChord(_))));
                } else if row.ambiguous.contains(canonical) {
                    assert!(matches!(result, Err(KeycapError::AmbiguousChord(_))));
                    conflicts += 1;
                } else {
                    let label = result.unwrap();
                    assert_eq!(label.canonical, *canonical);
                    assert!(!label.text.is_empty());
                }
            }
        }
        assert!(conflicts > 0);
    }
    #[test]
    fn i50_ambig_legend_keeps_only_the_ambiguous_part_canonical() {
        for (layout, ambiguous, relabelled, keycap) in
            [("de", "-", "[", "ü"), ("fr", "-", ",", ";")]
        {
            let s = snapshot(layout, 3);
            assert!(s.ambiguous().contains(&ambiguous));
            assert_eq!(
                s.label(ambiguous),
                Err(KeycapError::AmbiguousChord(ambiguous.into()))
            );
            assert_eq!(s.legend(ambiguous), Ok(ambiguous));
            assert_eq!(s.legend(relabelled), Ok(keycap));
            let label =
                chord_legend(&format!("{relabelled} {ambiguous} / {ambiguous}"), &s).unwrap();
            assert_eq!(label.text, format!("{keycap} {ambiguous} / {ambiguous}"));
            assert_eq!(
                label.canonical,
                format!("{relabelled} {ambiguous} / {ambiguous}")
            );
            assert_eq!(label.layout_generation, 3);
            assert!(chord_label(&label.canonical, &s).is_err());
            assert!(chord_legend("ctrl-wrong-key", &s).is_err());
        }
        let jp = snapshot("jp", 4);
        assert_eq!(jp.legend("+"), Ok("+"));
        assert_eq!(jp.legend("~"), Ok("~"));
        for row in KEYCAP_LAYOUTS {
            let s = KeycapSnapshot::new(row.layout, Some(row.variant), None, 5).unwrap();
            for (canonical, legend) in KEYCAP_DEFAULTS {
                match chord_label(canonical, &s) {
                    Err(KeycapError::AmbiguousChord(_)) => {
                        assert_eq!(s.legend(canonical), Ok(*legend))
                    }
                    result => assert_eq!(chord_legend(canonical, &s), result),
                }
            }
        }
    }

    #[test]
    fn i50_keycap_printed_shift_paths_are_available() {
        // jp AE12 is [asciicircum, asciitilde]: "~" is Shift on the "^" key, printed as "~".
        let s = KeycapSnapshot::new("jp", Some(""), None, 11).unwrap();
        assert_eq!(s.label("~"), Ok("~"));
        assert_eq!(chord_label("~", &s).unwrap().text, "~");
    }
}
