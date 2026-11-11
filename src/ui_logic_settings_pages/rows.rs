// One Settings pane line and the cursor rules over a list of them, from v0.3.12's ui/js/Settings.js.
use super::Text;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Group,
    Hint,
    Fact,
    Lock,
    Check,
    Choice,
    Action,
    Favourite,
    Ruler,
    KeyPreview,
    Hero,
}

// The colour a value or hint takes; None is the row's own muted ink.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    None,
    Live,
    Accent,
    Error,
    Foreground,
}

// A group master's tri-state, derived from its rows and never stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Master {
    #[default]
    Absent,
    All,
    Some,
    None,
}

// One example on the Keys pane: keys as keys.toml spells them, a catalog word for prose keys (the arrows), and what they do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyItem {
    pub keys: String,
    pub word: Text,
    pub label: Text,
    pub glyph: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    pub id: String,
    pub label: Text,
    pub glyph: &'static str,
    pub mark: &'static str,
    pub value: Text,
    pub caption: Text,
    pub role: Role,
    pub on: bool,
    pub available: bool,
    pub master: bool,
    pub state: Master,
    pub ids: Vec<&'static str>,
    pub values: Vec<String>,
    pub labels: Vec<Text>,
    pub selected: String,
    pub stops: Vec<u32>,
    pub index: usize,
    pub items: Vec<KeyItem>,
    pub footer: bool,
    pub url: &'static str,
    pub elide_head: bool,
    pub elide_right: bool,
    pub inert: bool,
    pub action: &'static str,
    pub error: Text,
    pub display: String,
    pub favourite: Option<usize>,
}

impl Default for Row {
    fn default() -> Self {
        Self {
            kind: Kind::Group, id: String::new(), label: Text::Empty, glyph: "", mark: "",
            value: Text::Empty, caption: Text::Empty, role: Role::None, on: false, available: true,
            master: false, state: Master::Absent, ids: Vec::new(), values: Vec::new(), labels: Vec::new(),
            selected: String::new(), stops: Vec::new(), index: 0, items: Vec::new(), footer: false, url: "",
            elide_head: false, elide_right: false, inert: false, action: "", error: Text::Empty, display: String::new(), favourite: None,
        }
    }
}

impl Row {
    pub fn new(kind: Kind, label: Text) -> Self {
        Self { kind, label, ..Self::default() }
    }

    pub fn group(label: &'static str) -> Self {
        Self::new(Kind::Group, Text::Key(label))
    }

    pub fn hint(label: Text) -> Self {
        Self::new(Kind::Hint, label)
    }

    pub fn check(id: &str, label: &'static str, glyph: &'static str, on: bool) -> Self {
        Self { id: id.into(), glyph, on, ..Self::new(Kind::Check, Text::Key(label)) }
    }

    pub fn fact(label: &'static str, value: Text) -> Self {
        Self { value, ..Self::new(Kind::Fact, Text::Key(label)) }
    }

    pub fn action(id: &str, label: &'static str, glyph: &'static str, value: Text) -> Self {
        Self { id: id.into(), glyph, value, ..Self::new(Kind::Action, Text::Key(label)) }
    }
}

// Choice values stay apart from labels, so presentation never becomes a persistence format.
pub fn choice(id: &str, label: &'static str, glyph: &'static str, values: &[&str], labels: &[&'static str], value: &str) -> Row {
    let at = values.iter().position(|v| *v == value).unwrap_or(0);
    Row {
        id: id.into(), glyph,
        values: values.iter().map(|v| (*v).to_owned()).collect(),
        labels: labels.iter().map(|l| Text::Key(l)).collect(),
        value: Text::Key(labels[at]),
        selected: value.to_owned(),
        ..Row::new(Kind::Choice, Text::Key(label))
    }
}

// Group, hint, fact and lock rows are read-only, and a greyed dependent is no stop either.
pub fn focusable(row: &Row) -> bool {
    if !row.available {
        return false;
    }
    match row.kind {
        Kind::Group => row.master || !row.action.is_empty(),
        Kind::Ruler => row.on,
        Kind::Check | Kind::Choice | Kind::Action | Kind::Favourite => true,
        _ => false,
    }
}

// Sample input: step_row(rows, 2, 1) is the next focusable index after 2, or 2 when none is.
pub fn step_row(rows: &[Row], from: usize, delta: isize) -> usize {
    let mut at = from as isize + delta;
    while at >= 0 && (at as usize) < rows.len() {
        if focusable(&rows[at as usize]) {
            return at as usize;
        }
        at += delta;
    }
    from
}

pub fn first_row(rows: &[Row]) -> usize {
    if rows.first().is_some_and(focusable) { 0 } else { step_row(rows, 0, 1) }
}
