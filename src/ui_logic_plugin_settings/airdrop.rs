//! The AirDrop plugin's page (board 0401-airdrop-page): Receiving, where received files go, and what this computer can do.
use super::pages::Row;
use super::wire::Plugin;
use crate::i18n::I18n;
use crate::ui_logic_send_share::airdrop::{adapter_value, Adapter};

pub const RECEIVING_ID: &str = "airdrop.receiving";
pub const FOLDER_ID: &str = "airdrop.folder";
pub const RECEIVING: [&str; 2] = ["off", "everyone"];

// What the window knows that the plugin listing does not: the radio, the folder in effect, the doctor's answer and the stack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub receiving: bool,
    pub folder: String,
    pub shown_as: String,
    pub adapter: Adapter,
    // Omdrop runs AirDrop in place of owly, so the credit names it.
    pub omdrop: bool,
    // Omdrop's saved Contacts Only, which its own `omdrop on` keeps.
    pub contacts: bool,
}

// The folder the page saved, when it names one.
pub fn saved_folder(plugin: &Plugin) -> Option<String> {
    plugin
        .settings
        .iter()
        .find_map(|(key, value)| match (key.as_str(), value) {
            ("folder", super::schema::Value::Text(path)) if path.starts_with('/') => {
                Some(path.clone())
            }
            _ => None,
        })
}

// The folder's own name in the value column, as the board's "Downloads".
fn leaf(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

// Omdrop keeps its saved audience and window length, so its on choice names only the audience.
fn receiving_labels(page: &Page, i18n: &I18n) -> Vec<String> {
    let on = match (page.omdrop, page.contacts) {
        (false, _) => "plugins.airdrop.receiving.everyone",
        (true, false) => "plugins.airdrop.receiving.omdrop_everyone",
        (true, true) => "plugins.airdrop.receiving.omdrop_contacts",
    };
    vec![i18n.tr("plugins.airdrop.receiving.off"), i18n.tr(on)]
}

pub fn rows(plugin: &Plugin, page: &Page, i18n: &I18n) -> Vec<Row> {
    let on = plugin.enabled;
    let labels = receiving_labels(page, i18n);
    vec![
        Row::Group {
            id: format!("{}:enabled", plugin.manifest.id),
            label: super::pages::name(plugin, i18n),
            switch: Some(on),
            action: None,
        },
        Row::Choice {
            id: RECEIVING_ID.into(),
            label: i18n.tr("plugins.airdrop.receiving"),
            glyph: "airdrop".into(),
            values: RECEIVING.iter().map(|v| v.to_string()).collect(),
            labels,
            selected: RECEIVING[usize::from(page.receiving)].into(),
            // Readiness gates starting; a receiver already running can always be turned off.
            available: on && (page.adapter.ready() || page.receiving),
        },
        Row::Action {
            id: FOLDER_ID.into(),
            label: i18n.tr("plugins.airdrop.folder"),
            glyph: "download".into(),
            value: leaf(&page.folder),
            chevron: true,
        },
        Row::Group {
            id: "airdrop.computer".into(),
            label: i18n.tr("plugins.airdrop.computer"),
            switch: None,
            action: None,
        },
        Row::Fact {
            text: i18n.tr("plugins.airdrop.shown_as"),
            value: page.shown_as.clone(),
        },
        Row::Fact {
            text: i18n.tr("plugins.airdrop.adapter"),
            value: adapter_value(&page.adapter, i18n),
        },
    ]
}

// The stack the plugin builds on, named on its page (proposal board 0401-airdrop-credit).
pub fn credit_rows(page: &Page, i18n: &I18n) -> Vec<Row> {
    let (stack, author) = match page.omdrop {
        true => ("Omdrop", "Brent Kearney (@netmojo)"),
        false => ("owly", "Tristan (@t4t5)"),
    };
    vec![
        Row::Group {
            id: "airdrop.credits".into(),
            label: i18n.tr("plugins.airdrop.credits"),
            switch: None,
            action: None,
        },
        Row::Fact {
            text: i18n.tr_args("plugins.airdrop.credit", &[stack, author]),
            value: "MIT".into(),
        },
    ]
}

#[cfg(test)]
#[path = "airdrop_tests.rs"]
mod tests;
