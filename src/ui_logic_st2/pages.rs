//! The rows Settings draws for Plugins: the installed list, one plugin's page and the Add card's state.
use super::builtin;
use super::schema::{Kind, Value};
use super::wire::{Plugin, Source};
use crate::i18n::I18n;

// One Settings row as plain data; the Settings host paints every kind in its own row look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    // A heading; switch is a master checkbox with its on or off word, action a flat plus and caption on the title row.
    Group { id: String, label: String, switch: Option<bool>, action: Option<String> },
    // The group rule alone, which sets the page's last row apart.
    Rule,
    // The muted line under the pane's title.
    Description { text: String },
    // A plugin in the installed list: its mark, name, enable checkbox and chevron, with a state word in error ink when it failed.
    Plugin { id: String, name: String, glyph: String, mark: String, on: bool, failure: Option<String> },
    Check { id: String, label: String, glyph: String, on: bool, available: bool },
    Choice { id: String, label: String, glyph: String, values: Vec<String>, labels: Vec<String>, selected: String, available: bool },
    Number { id: String, label: String, value: i64, min: i64, max: i64, step: i64, unit: String, available: bool },
    Field { id: String, label: String, value: String, placeholder: String, available: bool },
    Hint { text: String },
    // A muted fact, one per ACCESS entry, and the source line with its origin in the value column.
    Fact { text: String, value: String },
    // A pressable row: its value in the value column, a chevron when Enter does something.
    Action { id: String, label: String, glyph: String, value: String, chevron: bool },
    // A pinned shelf entry, as the favourites list draws one.
    Favourite { id: String, label: String, path: String, glyph: String, error: String, available: bool },
}

impl Row {
    pub fn id(&self) -> &str {
        match self {
            Row::Group { id, .. } | Row::Plugin { id, .. } | Row::Check { id, .. } | Row::Choice { id, .. }
            | Row::Number { id, .. } | Row::Field { id, .. } | Row::Action { id, .. } | Row::Favourite { id, .. } => id,
            Row::Rule | Row::Description { .. } | Row::Hint { .. } | Row::Fact { .. } => "",
        }
    }

    // Rows the keyboard cursor stops on; headings stop only when they carry a switch or an action.
    pub fn focusable(&self) -> bool {
        match self {
            Row::Group { switch, action, .. } => switch.is_some() || action.is_some(),
            Row::Rule | Row::Description { .. } | Row::Hint { .. } | Row::Fact { .. } => false,
            _ => true,
        }
    }
}

// What the Update row shows; v0.3.12's Update Flea words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Update {
    #[default]
    Idle,
    Checking,
    Current(String),
    Failed,
}

// The INSTALLED heading, whose Add Plugin… opens the Add card.
pub const ADD_ID: &str = "plugins.installed";
pub const BACK_ID: &str = "plugins.back";
pub const UPDATE_ID: &str = "plugins.update";
pub const REMOVE_ID: &str = "plugins.remove";

fn suffix(id: &str) -> &str {
    id.strip_prefix(super::manifest::RESERVED_PREFIX).unwrap_or(id)
}

// A built-in's name and description are Flea's own words, so they follow Flea's catalogs.
pub fn name(plugin: &Plugin, i18n: &I18n) -> String {
    match plugin.source {
        Source::Builtin => i18n.tr(&format!("plugins.builtin.{}.name", suffix(&plugin.manifest.id))),
        _ => plugin.manifest.name.clone(),
    }
}

pub fn description(plugin: &Plugin, i18n: &I18n) -> String {
    match plugin.source {
        Source::Builtin => i18n.tr(&format!("plugins.builtin.{}.description", suffix(&plugin.manifest.id))),
        _ => plugin.manifest.description.clone(),
    }
}

pub fn failure_word(plugin: &Plugin, i18n: &I18n) -> Option<String> {
    plugin.failure.as_ref().map(|_| i18n.tr("plugins.state.failed"))
}

pub fn list_rows(plugins: &[Plugin], i18n: &I18n) -> Vec<Row> {
    let mut rows = vec![Row::Group {
        id: ADD_ID.into(), label: i18n.tr("plugins.installed"), switch: None, action: Some(i18n.tr("plugins.add")),
    }];
    for p in plugins {
        rows.push(Row::Plugin {
            id: p.manifest.id.clone(), name: name(p, i18n), glyph: p.manifest.glyph.clone(), mark: p.mark_path.clone(),
            on: p.enabled, failure: failure_word(p, i18n),
        });
    }
    rows
}

// Sample input: ["sha256sum","sha1sum","md5sum"] gives "sha256sum, sha1sum or md5sum".
fn joined(items: &[&str], i18n: &I18n) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => i18n.tr_args("plugins.list.or", &[&rest.join(&i18n.tr("plugins.list.comma")), last]),
    }
}

// Each declared access is one plain line; every exec: entry folds into one "Runs" line where the first one stood.
pub fn access_lines(access: &[String], i18n: &I18n) -> Vec<String> {
    let programs: Vec<&str> = access.iter().filter_map(|a| a.strip_prefix("exec:")).filter(|p| !p.is_empty()).collect();
    let mut lines = Vec::new();
    let mut ran = false;
    for entry in access {
        let key = match entry.as_str() {
            "selection" => "plugins.access.selection",
            "write:selection" => "plugins.access.write_selection",
            "folder" => "plugins.access.folder",
            "network" => "plugins.access.network",
            "notify" => "plugins.access.notify",
            "shelf:pile" => "plugins.access.shelf_pile",
            "shelf:bar" => "plugins.access.shelf_bar",
            "shelf:captures" => "plugins.access.shelf_captures",
            "airdrop:send" => "plugins.access.airdrop_send",
            "airdrop:wifi" => "plugins.access.airdrop_wifi",
            "airdrop:receive" => "plugins.access.airdrop_receive",
            e if e.starts_with("exec:") && !ran && !programs.is_empty() => {
                ran = true;
                lines.push(i18n.tr_args("plugins.access.exec", &[&joined(&programs, i18n)]));
                continue;
            }
            // Later exec: entries are already in the one Runs line.
            e if e.starts_with("exec:") => continue,
            // An entry this Flea cannot name in plain words is still shown, as the plugin wrote it.
            other => {
                lines.push(i18n.tr_args("plugins.access.other", &[other]));
                continue;
            }
        };
        lines.push(i18n.tr(key));
    }
    lines
}

fn schema_rows(plugin: &Plugin) -> Vec<Row> {
    let mut rows = Vec::new();
    for field in &plugin.manifest.schema {
        let value = plugin.settings.iter().find(|(k, _)| *k == field.key).map(|(_, v)| v.clone());
        let id = format!("{}:{}", plugin.manifest.id, field.key);
        let (label, available) = (field.label.clone(), plugin.enabled);
        rows.push(match (&field.kind, value) {
            (Kind::Boolean { .. }, Some(Value::Bool(on))) => Row::Check { id, label, glyph: String::new(), on, available },
            (Kind::Integer { min, max, step, unit, .. }, Some(Value::Int(value))) => {
                Row::Number { id, label, value, min: *min, max: *max, step: *step, unit: unit.clone(), available }
            }
            (Kind::Text { placeholder, .. }, Some(Value::Text(value))) => {
                Row::Field { id, label, value, placeholder: placeholder.clone(), available }
            }
            (Kind::Choice { options, .. }, Some(Value::Choice(selected))) => Row::Choice {
                id, label, glyph: String::new(), values: options.iter().map(|(v, _)| v.clone()).collect(),
                labels: options.iter().map(|(_, l)| l.clone()).collect(), selected, available,
            },
            // The listing always carries a clamped value of the field's own type, so this arm is a malformed listing.
            _ => continue,
        });
        if !field.hint.is_empty() {
            rows.push(Row::Hint { text: field.hint.clone() });
        }
    }
    rows
}

fn update_value(update: &Update, i18n: &I18n) -> (String, bool) {
    match update {
        Update::Idle => (i18n.tr("plugins.update.check"), true),
        Update::Checking => (i18n.tr("plugins.update.checking"), false),
        Update::Current(version) => (i18n.tr_args("plugins.update.current", &[version]), true),
        Update::Failed => (i18n.tr("plugins.update.failed"), true),
    }
}

// The plugin's page: description, its heading carrying the enable, its settings, ACCESS, SOURCE when added, and Back.
pub fn page_rows(plugin: &Plugin, update: &Update, i18n: &I18n) -> Vec<Row> {
    page_rows_with(plugin, update, None, i18n)
}

// The same page with the window's AirDrop facts, which only the AirDrop plugin's page draws.
pub fn page_rows_with(plugin: &Plugin, update: &Update, airdrop: Option<&super::airdrop::Page>, i18n: &I18n) -> Vec<Row> {
    let mut rows = vec![Row::Description { text: description(plugin, i18n) }];
    let id = &plugin.manifest.id;
    let airdrop = airdrop.filter(|_| *id == builtin::AIRDROP);
    match (&plugin.shelf, airdrop) {
        (Some(input), _) => rows.extend(super::shelf::rows(input, i18n)),
        (None, Some(page)) => rows.extend(super::airdrop::rows(plugin, page, i18n)),
        _ => {
            rows.push(Row::Group { id: format!("{id}:enabled"), label: name(plugin, i18n), switch: Some(plugin.enabled), action: None });
            rows.extend(schema_rows(plugin));
        }
    }
    let access = access_lines(&plugin.manifest.access, i18n);
    if !access.is_empty() {
        rows.push(Row::Group { id: "plugins.access".into(), label: i18n.tr("plugins.access.heading"), switch: None, action: None });
        rows.extend(access.into_iter().map(|text| Row::Fact { text, value: String::new() }));
    }
    if airdrop.is_some() {
        rows.extend(super::airdrop::credit_rows(i18n));
    }
    let origin = match &plugin.source {
        Source::Builtin => None,
        Source::Git(origin) | Source::Folder(origin) => Some(origin.as_str()),
        Source::Unknown => Some(""),
    };
    if let Some(origin) = origin {
        rows.push(Row::Group { id: "plugins.source".into(), label: i18n.tr("plugins.source.heading"), switch: None, action: None });
        if !origin.is_empty() {
            rows.push(Row::Fact { text: i18n.tr("plugins.source.from"), value: shown_origin(origin) });
        }
        if matches!(plugin.source, Source::Git(_)) {
            let (value, chevron) = update_value(update, i18n);
            rows.push(Row::Action { id: UPDATE_ID.into(), label: i18n.tr("plugins.update.label"), glyph: "download".into(), value, chevron });
        }
        rows.push(Row::Action { id: REMOVE_ID.into(), label: i18n.tr("plugins.remove.label"), glyph: String::new(), value: String::new(), chevron: false });
    }
    rows.push(Row::Rule);
    rows.push(Row::Action { id: BACK_ID.into(), label: i18n.tr("plugins.back.label"), glyph: String::new(), value: i18n.tr("plugins.back.value"), chevron: true });
    rows
}

// Sample input: "https://github.com/mtorvik/flea-checksums.git" shows as "github.com/mtorvik/flea-checksums".
pub fn shown_origin(origin: &str) -> String {
    let bare = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    let bare = bare.strip_prefix("git@").map(|r| r.replacen(':', "/", 1)).unwrap_or_else(|| bare.to_string());
    bare.trim_end_matches('/').trim_end_matches(".git").to_string()
}

// The plugins whose rows, rail sections and send targets other surfaces draw: enabled and started; any other draws nothing.
pub fn contributing(plugins: &[Plugin]) -> impl Iterator<Item = &Plugin> {
    plugins.iter().filter(|p| p.enabled && p.failure.is_none())
}

pub fn is_builtin(plugin: &Plugin) -> bool {
    builtin::is_builtin(&plugin.manifest.id)
}

// The Add card: Add is the default and stays unavailable until something is typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddCard {
    pub text: String,
    pub busy: bool,
    pub error: Option<String>,
}

impl AddCard {
    pub fn can_add(&self) -> bool {
        !self.busy && !self.text.trim().is_empty()
    }
}
