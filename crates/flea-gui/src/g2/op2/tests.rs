//! Compress, extract-password, flash and extracting board states as plain data.
use super::cards::{Block, Button, Card, Ink};
use super::Effect;
use flea::i18n::{I18n, Paths};

pub(super) fn language(file: &str) -> I18n {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/ui_logic_op2/fixtures");
    I18n::load(Paths {
        omarchy: fixtures.clone(),
        language_file: fixtures.join(file),
        catalogs: Vec::new(),
    })
}

pub(super) fn english() -> I18n {
    language("/nonexistent/language")
}

pub(super) fn buttons(card: &Card) -> String {
    card.buttons
        .iter()
        .map(|b: &Button| {
            format!(
                "{}{}{}{}{}",
                b.label,
                if b.primary { "*" } else { "" },
                if b.focused { "@" } else { "" },
                if b.enabled { "" } else { "~" },
                if b.available { "" } else { "!" },
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) fn notes(card: &Card) -> Vec<(String, Ink)> {
    card.body
        .iter()
        .filter_map(|b| match b {
            Block::Note { text, ink } => Some((text.clone(), *ink)),
            _ => None,
        })
        .collect()
}

pub(super) fn fields(card: &Card) -> Vec<(String, String, bool, bool)> {
    card.body
        .iter()
        .filter_map(|b| match b {
            Block::Field(field) => Some((
                field.label.clone().unwrap_or_default(),
                field.drawn(),
                field.focused,
                field.error,
            )),
            _ => None,
        })
        .collect()
}

pub(super) fn sent(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Send(line) => Some(line.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn said(effects: &[Effect]) -> Vec<(String, bool)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Say { text, error } => Some((text.clone(), *error)),
            _ => None,
        })
        .collect()
}
