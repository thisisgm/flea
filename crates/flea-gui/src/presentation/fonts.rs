use crate::list::Typeface;
use flea::{jsondoc::Json, ui_logic_settings_pages::textsize};
use gpui::{font, px, Global};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fonts {
    pub family: String,
    pub base: u32,
    pub body: u32,
    pub body_small: u32,
    pub caption: u32,
}

impl Global for Fonts {}

impl Fonts {
    pub fn from_sources(theme: &str, user: &str, family: String) -> Self {
        let mut values = BTreeMap::new();
        for text in [theme, user] {
            let mut section = "";
            for line in text.lines() {
                let line = uncomment(line).trim();
                if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    section = name;
                } else if section == "font" {
                    if let Some((key, value)) = line.split_once('=') {
                        if let Some(value) = shell_value(value.trim()) {
                            values.insert(key.trim(), value);
                        }
                    }
                }
            }
        }
        let number = |key| values.get(key).and_then(|s| integer(s));
        let base = number("base-size").unwrap_or(12).max(1) as u32;
        let token = |key, fallback| {
            number(key)
                .filter(|n| *n > 0)
                .map_or(fallback, |n| n as u32)
        };
        Self {
            family,
            base,
            body: token("body", textsize::body(base)),
            body_small: token("body-small", textsize::body_small(base)),
            caption: token("caption", textsize::caption(base)),
        }
    }

    pub fn stored(settings: &Json) -> textsize::Stored {
        textsize::parse(
            settings
                .get("display")
                .and_then(|d| d.get("textSize"))
                .unwrap_or(&Json::Null),
        )
    }

    pub fn typeface(&self, settings: &Json) -> Typeface {
        let stored = Self::stored(settings);
        let overridden = !textsize::following(stored);
        let base = textsize::effective(stored, self.base);
        Typeface {
            font: font(self.family.clone()),
            base: px(base as f32),
            body: px(if overridden {
                textsize::body(base)
            } else {
                self.body
            } as f32),
            body_small: px(if overridden {
                textsize::body_small(base)
            } else {
                self.body_small
            } as f32),
            caption: px(if overridden {
                textsize::caption(base)
            } else {
                self.caption
            } as f32),
        }
    }
}

fn integer(value: &str) -> Option<i64> {
    let length = value
        .bytes()
        .enumerate()
        .take_while(|(i, b)| b.is_ascii_digit() || (*i == 0 && matches!(*b, b'-' | b'+')))
        .count();
    value.get(..length)?.parse().ok()
}

pub(crate) fn uncomment(line: &str) -> &str {
    let mut quote = None;
    for (at, ch) in line.char_indices() {
        match (quote, ch) {
            (None, '\'' | '"') => quote = Some(ch),
            (Some(open), close) if open == close => quote = None,
            (None, '#') => return &line[..at],
            _ => {}
        }
    }
    line
}

pub(crate) fn shell_value(raw: &str) -> Option<&str> {
    if raw.starts_with(['\'', '"']) {
        let quote = raw.as_bytes()[0];
        return (raw.len() > 2
            && raw.as_bytes().last() == Some(&quote)
            && !raw[1..raw.len() - 1].contains(['\'', '"']))
        .then(|| &raw[1..raw.len() - 1]);
    }
    let number = raw.strip_prefix('-').unwrap_or(raw);
    let mut parts = number.split('.');
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let numeric = parts.next().is_some_and(digits)
        && parts.next().is_none_or(digits)
        && parts.next().is_none();
    let bare = raw.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
    (numeric || bare).then_some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_user_keys_override_theme_keys_and_pinned_size_uses_the_shared_ladder() {
        let fonts = Fonts::from_sources(
            "[font]\nbase-size=16\nbody-small=17",
            "[font]\nbase-size=12\ncaption=11",
            "monospace".into(),
        );
        let following = fonts.typeface(&Json::Null);
        assert_eq!(
            (
                f32::from(following.body),
                f32::from(following.body_small),
                f32::from(following.caption)
            ),
            (12., 17., 11.)
        );
        let pinned = fonts
            .typeface(&flea::jsondoc::parse(r#"{"display":{"textSize":{"mode":14}}}"#).unwrap());
        assert_eq!(
            (
                f32::from(pinned.body),
                f32::from(pinned.body_small),
                f32::from(pinned.caption)
            ),
            (14., 13., 12.)
        );
        let invalid = Fonts::from_sources(
            "[font]\nbase-size=16\nbody=18",
            "[font]\nbase-size=bad\nbody=-2",
            "monospace".into(),
        );
        assert_eq!((invalid.base, invalid.body), (12, 12));
        let separate = Fonts::from_sources("[font]\nbase-size=14\nbody=18", "", "monospace".into())
            .typeface(&Json::Null);
        assert_eq!(
            crate::list::Metrics::for_typeface(&separate).row,
            crate::list::Metrics::at(px(14.)).row
        );
    }
}
