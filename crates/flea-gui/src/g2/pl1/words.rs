//! Notes rendered through the catalog: the only place a device sentence becomes text.
use flea::i18n::I18n;
use flea::ui_logic_pl1::Note;

/// The note in the active language; a plain note is data and passes through as is.
pub fn say(catalog: &I18n, note: &Note) -> String {
    let args: Vec<&str> = note.args.iter().map(String::as_str).collect();
    match (note.key, note.count) {
        ("", _) => args.join(" "),
        (key, Some(count)) => catalog.trn_args(key, count, &args),
        (key, None) if args.is_empty() => catalog.tr(key),
        (key, None) => catalog.tr_args(key, &args),
    }
}
