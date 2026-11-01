//! Card errors retain shared semantic words until cleared or replaced.
use crate::{i18n::I18n, tui::words::Text};

#[derive(Clone, Debug)]
pub struct Held {
    pub text: Text,
    shown: String,
}
impl Held {
    pub fn set(slot: &mut String, held: &mut Option<Self>, text: Text, i18n: &I18n) {
        *slot = text.render(i18n);
        *held = Some(Self {
            text,
            shown: slot.clone(),
        });
    }
    pub fn refresh(slot: &mut String, held: &mut Option<Self>, i18n: &I18n) {
        if let Some(word) = held {
            if *slot == word.shown {
                *slot = word.text.render(i18n);
                word.shown.clone_from(slot);
                return;
            }
        }
        *held = None;
    }
}
