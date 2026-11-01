//! Keycaps in the active language: Omarchy's own key words in every language, isolated LRI..PDI inside right-to-left text.
use super::{hinted, hinted_all, key_for};
use crate::i18n::{I18n, Paths};
use crate::ui_logic_st1::Text;
use std::path::Path;

const LRI: char = '\u{2066}';
const FSI: char = '\u{2068}';
const PDI: char = '\u{2069}';
const SIZES: [&str; 3] = ["textSizeUp", "textSizeDown", "textSizeReset"];

// ru is non-Latin and left-to-right; fa and ar are right-to-left, ar's catalog already wraps each %N in FSI..PDI.
pub(crate) fn language(code: &str) -> I18n {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/keys/fixtures/keycaps");
    I18n::load(Paths {
        omarchy: root.clone(),
        language_file: root.join(format!("language-{code}")),
        catalogs: vec![root.join("catalogs")],
    })
}

#[test]
fn i50_keycap_words_stay_omarchys_and_rtl_isolates_them() {
    let undo = key_for("default", "undo");
    let [up, down, reset] = SIZES.map(|action| key_for("default", action));
    assert_eq!(
        (up.as_str(), down.as_str(), reset.as_str()),
        ("Ctrl+Shift++", "Ctrl+Shift+-", "Ctrl+Shift+0")
    );
    let ru = language("ru");
    assert_eq!(ru.language(), "ru");
    assert_eq!(
        hinted(&ru, "feedback.hint.undo", "default", "undo"),
        format!("{undo} отменяет действие")
    );
    assert_eq!(
        hinted_all(&ru, "settings.hint.text_keys", "default", &SIZES),
        "Ctrl+Shift++ и Ctrl+Shift+- меняют размер по делениям; Ctrl+Shift+0 возвращает размер Omarchy."
    );
    let fa = language("fa");
    assert!(fa.locale().right_to_left);
    assert_eq!(
        hinted(&fa, "feedback.hint.undo", "default", "undo"),
        format!("{LRI}{undo}{PDI} واگرد می‌کند")
    );
    assert_eq!(
        hinted_all(&fa, "settings.hint.text_keys", "default", &SIZES),
        format!("{LRI}{up}{PDI} و {LRI}{down}{PDI} بین اندازه‌ها حرکت می‌کنند؛ {LRI}{reset}{PDI} دوباره از Omarchy پیروی می‌کند.")
    );
    let ar = language("ar");
    assert_eq!(
        hinted(&ar, "feedback.hint.undo", "default", "undo"),
        format!("{FSI}{LRI}{undo}{PDI}{PDI} للتراجع")
    );
    // A preset that binds no key still leaves the sentence out, isolated or not.
    assert_eq!(fa.keycap(""), "");
    assert_eq!(
        hinted_all(
            &fa,
            "settings.hint.text_keys",
            "default",
            &["textSizeUp", "noSuchAction"]
        ),
        ""
    );
}

#[test]
fn i50_keycap_settings_text_resolves_in_the_active_language() {
    let load = Text::args(
        "settings.hint.load_key",
        vec![Text::Keycap("Ctrl+Space".into())],
    );
    assert_eq!(
        load.resolve(&language("ru")),
        "Ctrl+Space загружает текущее выделение."
    );
    assert_eq!(
        load.resolve(&language("fa")),
        format!("{LRI}Ctrl+Space{PDI} انتخاب فعلی را بارگذاری می‌کند.")
    );
    assert!(!load.is_empty() && Text::Keycap(String::new()).is_empty());
    // Data is a name or a number, never a keycap: it stays as given.
    let px = Text::args(
        "settings.textsize.override",
        vec![Text::data("14px"), Text::Keycap("Ctrl+0".into())],
    );
    assert_eq!(
        px.resolve(&language("fa")),
        format!("اندازهٔ متن 14px. {LRI}Ctrl+0{PDI} دوباره از Omarchy پیروی می‌کند")
    );
}
