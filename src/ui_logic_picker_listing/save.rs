//! Save-name checks mirroring backend valid_name; sentences via keys.
use crate::i18n::I18n;

// The save name is a trust boundary.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

// The stem ends at its last dot; a leading dot stays.
pub fn stem_end(name: &str) -> usize {
    match name.rfind('.') {
        Some(dot) if dot > 0 => dot,
        _ => name.len(),
    }
}

// A name is one token; zero-width joints let long names wrap.
pub fn joint_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        out.push(c);
        if c == '.' || c == '_' || c == '-' {
            out.push('\u{200b}')
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveCheck {
    NeedName,
    Refused,
    Exists,
    LocationChanged,
    // No probe answered yet; the window probes before writing.
    Probe,
    Ready,
}

// Check order: empty, refused, moved, probe; only Ready writes.
pub fn check_save_name(name: &str, probe: Option<bool>, location_changed: bool) -> SaveCheck {
    if name.is_empty() {
        return SaveCheck::NeedName;
    }
    if !valid_name(name) {
        return SaveCheck::Refused;
    }
    if location_changed {
        return SaveCheck::LocationChanged;
    }
    match probe {
        Some(true) => SaveCheck::Exists,
        Some(false) => SaveCheck::Ready,
        None => SaveCheck::Probe,
    }
}

// PickerSave's refused line: one template, the built-in rule as an argument.
pub fn save_refused(i18n: &I18n, name: &str) -> String {
    let rule = i18n.tr("picker.browser.name_refused");
    i18n.tr_args("picker.browser.save_refused", &[name, &rule])
}

// The collision review's field line; the caller passes the jointed name.
pub fn collision_short(i18n: &I18n, jointed: &str) -> String {
    i18n.tr_args("picker.browser.collision_short", &[jointed])
}

// The status line's sentence when Return asks about a taken name.
pub fn collision_review(i18n: &I18n, jointed: &str) -> String {
    i18n.tr_args("picker.browser.collision_review", &[jointed])
}

pub fn save_stopped(i18n: &I18n) -> String {
    i18n.tr("picker.browser.save_stopped")
}

// The field line when the backend dies.
pub fn save_stopped_field(i18n: &I18n) -> String {
    i18n.tr("picker.browser.save_stopped_field")
}

// Accept with no name: the footer asks first.
pub fn need_name(i18n: &I18n) -> String {
    i18n.tr("picker.browser.save_empty")
}

pub fn save_moved(i18n: &I18n) -> String {
    i18n.tr("picker.browser.save_moved")
}

// Listing failure lines: dead worker, unstarted worker, refusal.
pub fn list_failed(i18n: &I18n, state: &str) -> String {
    let key = match state {
        "dead" => "picker.browser.list_failed_gone",
        "unstarted" => "picker.browser.list_failed_restart",
        _ => "picker.browser.list_failed",
    };
    i18n.tr(key)
}

// Scan/sort failure says the backend msg via template.
pub fn list_failed_reason(i18n: &I18n, msg: &str) -> String {
    i18n.tr_args("picker.browser.list_failed_reason", &[msg])
}
