//! The launch text epoch: the roster's locale, not the process locale, chooses glyph faces before the first window draws.
use flea::i18n::I18n;
use gpui::{App, SharedString, TextEpochConfig};
use std::time::Duration;

// The fork refuses a tag longer than this, as no real BCP 47 tag is.
const MAX_TAG_LEN: usize = 128;

// The fork's own reading: "ja_JP.UTF-8@x" ranks as "ja-JP", and C, POSIX, nothing or no tag at all as "en-US".
pub(crate) fn tag(posix: &str) -> String {
    let tag = posix
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    let valid = !tag.is_empty()
        && tag.len() <= MAX_TAG_LEN
        && tag
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    if !valid || tag == "C" || tag == "POSIX" {
        "en-US".into()
    } else {
        tag
    }
}

// Only these languages share Han characters whose glyph forms the locale picks; any other pair draws the same faces.
const HAN_FORMS: [&str; 3] = ["ja", "ko", "zh"];

// True when the two tags choose different faces: an installed epoch stops loading faces on demand, so it is paid only for a real change.
pub(crate) fn faces_differ(roster: &str, process: &str) -> bool {
    let han = |tag: &str| HAN_FORMS.contains(&tag.split('-').next().unwrap_or_default());
    roster != process && (han(roster) || han(process))
}

// The locale the text system was built with, read the way the fork reads it.
fn process_locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

// An installed snapshot loads no face later, so it holds an emoji face beside the roster's scripts for file names.
const HELD_EXTRA: &str = "\u{1F600}";

/// The longest a launch waits for the snapshot; past it the window opens with the process locale's faces, as before.
pub(crate) const LAUNCH_WAIT: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// The process locale already draws the faces the roster's locale would, so nothing was prepared.
    Same,
    Installed(usize),
    /// Preparation or installation failed or ran out of time, and the process locale's faces stay whole.
    Kept(String),
}

/// Installs the roster locale's text epoch before any window exists, so no window or cached line ever holds the other faces.
pub fn launch(catalog: &I18n, family: &str, cx: &mut App) -> Launch {
    launch_from(&process_locale(), catalog, family, LAUNCH_WAIT, cx)
}

/// The roster's own locale, the window family and every character the snapshot must resolve before it freezes.
pub(crate) fn config(catalog: &I18n, family: &str) -> TextEpochConfig {
    TextEpochConfig {
        locale: catalog.locale().glibc_locale.clone(),
        fonts: vec![gpui::font(SharedString::from(family.to_owned()))],
        text: vec![catalog.held_text().into(), HELD_EXTRA.into()],
        fontconfig_file: None,
    }
}

pub(crate) fn launch_from(
    process: &str,
    catalog: &I18n,
    family: &str,
    wait: Duration,
    cx: &mut App,
) -> Launch {
    let locale = &catalog.locale().glibc_locale;
    if !faces_differ(&tag(locale), &tag(process)) {
        return Launch::Same;
    }
    let config = config(catalog, family);
    let text = cx.text_system().clone();
    // fontconfig runs on the fork's epoch worker; this thread only waits, and only before a window exists.
    let outcome = text
        .prepare_text_epoch(config)
        .map_err(|error| error.to_string())
        .and_then(|task| {
            cx.foreground_executor()
                .block_with_timeout(wait, task.prepared())
                .map_err(|_| "text epoch timed out".to_owned())?
                .map_err(|error| error.to_string())
        })
        .and_then(|prepared| {
            text.install_text_epoch(&prepared)
                .map_err(|error| error.to_string())
        });
    match outcome {
        Ok(()) => Launch::Installed(text.text_epoch()),
        Err(error) => {
            eprintln!("flea-gui: text epoch for {locale}: {error}");
            Launch::Kept(error)
        }
    }
}
