//! Explicit US post-translation fixtures for the platform-independent rule tests.
use super::*;
use crate::keys::{Input, ALT, CONTROL, MODIFIER_MASK, SHIFT, SUPER};

pub(super) fn projection() -> Projection {
    let mut projection = Projection::default();
    let lower = "abcdefghijklmnopqrstuvwxyz0123456789";
    let upper = "ABCDEFGHIJKLMNOPQRSTUVWXYZ)!@#$%^&*(";
    for mask in 0..=MODIFIER_MASK {
        projection.modifiers.push(mask);
        for caps in [false, true] {
            for (low, high) in lower.chars().zip(upper.chars()) {
                let shifted = mask & SHIFT != 0;
                let take_upper = if low.is_ascii_alphabetic() {
                    shifted ^ caps
                } else {
                    shifted
                };
                let key = low.to_string();
                let text = if mask & (CONTROL | ALT | SUPER) != 0 {
                    String::new()
                } else if take_upper {
                    high.to_string()
                } else {
                    low.to_string()
                };
                projection.samples.push(
                    Sample::new([
                        Some(Input {
                            key: &key,
                            text: &text,
                            modifiers: mask,
                        }),
                        None,
                    ])
                    .unwrap(),
                );
            }
        }
        for key in ["Return", "Tab", "Escape", "Space"]
            .into_iter()
            .map(String::from)
            .chain((1..=24).map(|n| format!("F{n}")))
        {
            projection.samples.push(
                Sample::new([
                    Some(Input {
                        key: &key,
                        text: "",
                        modifiers: mask,
                    }),
                    None,
                ])
                .unwrap(),
            );
        }
    }
    projection
}

pub(super) fn find<'a>(
    keys: &'a UserKeys,
    preset: &str,
    input: Input<'_>,
    plugin: &dyn Fn(&str) -> bool,
) -> Option<&'a UserBinding> {
    let verdicts = keys.verdicts(preset, plugin, &projection());
    keys.find(preset, input, plugin, &verdicts)
}
