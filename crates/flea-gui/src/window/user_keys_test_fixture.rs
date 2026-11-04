//! Raw US fixtures feed the same sampler cache and translator as the real platform method.
use gpui::{Capslock, Keystroke, Modifiers};

pub(crate) fn samples(held: Modifiers, caps: Capslock) -> Option<Vec<(Keystroke, Modifiers)>> {
    let lower = "abcdefghijklmnopqrstuvwxyz1234567890-=[];'`\\,./";
    let upper = "ABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()_+{}:\"~|<>?";
    let mut samples = Vec::new();
    for (low, high) in lower.chars().zip(upper.chars()) {
        let letter = low.is_ascii_alphabetic();
        let us = if held.shift && !letter { high } else { low };
        let key = if letter { low } else { us };
        let typed = if held.shift ^ (letter && caps.on) {
            high
        } else {
            low
        };
        let mut normalized = held;
        if !letter {
            normalized.shift = false;
        }
        samples.push((
            Keystroke {
                key: key.to_string(),
                key_char: (!held.control && !held.alt && !held.platform).then(|| typed.to_string()),
                us_key: Some(us.to_string()),
                modifiers: normalized,
                is_keypad: false,
            },
            held,
        ));
    }
    for key in ["enter", "tab", "escape", "space"]
        .into_iter()
        .map(String::from)
        .chain((1..=24).map(|n| format!("f{n}")))
    {
        samples.push((
            Keystroke {
                key,
                modifiers: held,
                ..Default::default()
            },
            held,
        ));
    }
    Some(samples)
}

pub(crate) fn seed(shell: &mut crate::state::Shell, cx: &gpui::Context<crate::state::Shell>) {
    shell.user_keys.layout.refresh(
        cx.keyboard_layout().id(),
        0..=flea::keys::MODIFIER_MASK,
        samples,
    );
}
