//! The unlock card's words, built once per card change or language change.
use super::unlock::Card;
use super::words::say;
use crate::g2::status_feedback::caption::heading_for;
use flea::i18n::I18n;
use flea::ui_logic_device_rail::unlock::{subtitle, title};

/// The words the card shows, built once per card change or language change, never in paint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardText {
    pub language: String,
    pub title: String,
    pub esc: String,
    pub subtitle: String,
    pub label: String,
    pub note: String,
    pub cancel: String,
    pub unlock: String,
}

impl CardText {
    pub fn of(card: &Card, catalog: &I18n) -> Self {
        let size = if card.size > 0 {
            catalog.locale().format_size(card.size)
        } else {
            String::new()
        };
        // BitLocker names its secrets as Windows does: a password, or the 48-digit recovery key.
        let bitlocker = card.crypto == "BitLocker";
        let label = if bitlocker {
            catalog.tr("device.unlock.recovery_key")
        } else {
            catalog.tr("device.unlock.passphrase")
        };
        let wrong = card
            .note
            .as_ref()
            .is_some_and(|(n, _)| n.key == "device.unlock.wrong");
        let mut note = match &card.note {
            Some(_) if wrong && bitlocker => catalog.tr("device.unlock.wrong_bitlocker"),
            Some((n, _)) => say(catalog, n),
            None => String::new(),
        };
        // A refusal names Caps Lock on its own line when it was on as Unlock was pressed, however long the key took to refuse.
        if wrong && card.field.caps_on {
            note = format!("{note}\n{}", catalog.tr("device.unlock.caps_lock"));
        }
        Self {
            language: catalog.language().into(),
            title: say(catalog, &title(&card.label)),
            esc: "esc".into(),
            subtitle: say(catalog, &subtitle(&card.label, &size, &card.crypto)),
            label: heading_for(&label, catalog.language()),
            note,
            cancel: catalog.tr("device.unlock.cancel"),
            unlock: catalog.tr("device.unlock.button"),
        }
    }
}
