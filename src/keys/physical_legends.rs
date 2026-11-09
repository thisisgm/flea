//! The legends the active group prints on the drawn keyboard, read from GPUI's per-keycode samples.
use super::Cap;

/// One key the layout reports, unshifted and with Shift held, in the platform's keycode order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sample {
    pub key: String,
    pub us: Option<String>,
    pub text: Option<String>,
    pub shifted: Option<String>,
}

/// The one character a sample types, if it types exactly one.
fn single(text: Option<&str>) -> bool {
    text.is_some_and(|text| {
        text.chars().count() == 1 && text.chars().all(|c| !c.is_control() && !c.is_whitespace())
    })
}

/// The key beside left Shift reports no US position; it is the one printing key left without one.
fn lsgt(samples: &[Sample]) -> Option<usize> {
    samples
        .iter()
        .position(|sample| sample.us.is_none() && single(sample.text.as_deref()))
}

/// ISO when the key beside left Shift types something no main-block key does. xkb gives every `pc` group that key,
/// typing `<` on US, but a US board has no such key, and `<` there is Shift and comma.
pub fn is_iso(samples: &[Sample]) -> bool {
    let Some(extra) = lsgt(samples).and_then(|index| samples[index].text.as_deref()) else {
        return false;
    };
    !samples
        .iter()
        .filter(|sample| sample.us.is_some())
        .any(|sample| {
            sample.text.as_deref() == Some(extra) || sample.shifted.as_deref() == Some(extra)
        })
}

/// Right Alt is Alt where the group names it Alt_R; most groups make it AltGr (ISO_Level3_Shift).
pub fn right_alt_is_alt(samples: &[Sample]) -> bool {
    samples.iter().any(|sample| sample.key == "alt_r")
}

/// The sample a cap reads: by US position for a printing cap, by key name for a named one.
pub fn sample_of(cap: &Cap, samples: &[Sample]) -> Option<usize> {
    if cap.us == "<" {
        return lsgt(samples);
    }
    if !cap.us.is_empty() {
        return samples
            .iter()
            .position(|sample| sample.us.as_deref() == Some(cap.us));
    }
    if cap.key.is_empty() {
        return None;
    }
    samples.iter().position(|sample| sample.key == cap.key)
}

/// A cap's two legends: what it types, upper-cased for a letter as keycaps print it, and its shifted character when that differs.
pub fn legends(cap: &Cap, samples: &[Sample]) -> (String, String) {
    if cap.us.is_empty() {
        return (cap.name.into(), String::new());
    }
    let Some(sample) = sample_of(cap, samples).map(|index| &samples[index]) else {
        return (String::new(), String::new());
    };
    let text = sample.text.clone().unwrap_or_default();
    let shifted = sample.shifted.clone().unwrap_or_default();
    let upper = text.to_uppercase();
    if shifted == upper || shifted.is_empty() {
        (upper, String::new())
    } else {
        (text, shifted)
    }
}
