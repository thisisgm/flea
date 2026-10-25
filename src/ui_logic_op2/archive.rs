//! Which rows get an Extract and what a new archive is called; the backend sends no is-archive flag, so this is the mechanism.

// Longest form first, so ".tar.gz" matches before ".gz"; .rar is read here and never offered to write.
pub const EXTENSIONS: [&str; 9] =
    [".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar", ".zip", ".7z", ".rar"];
// The widened tail of EXTENSIONS, which 7z reads; tar and the zip class keep their own rules.
pub const SEVEN_TAILS: [&str; 8] =
    [".appimage", ".cpio", ".cab", ".jar", ".apk", ".rpm", ".deb", ".iso"];
// A disk image lists in the archive preview and is never extracted.
pub const LIST_ONLY: [&str; 1] = [".iso"];
// The board's chip order, 7z first, whatever order the backend reports its formats in.
pub const BOARD_ORDER: [&str; 2] = ["7z", "zip"];

// Sample input: "backup.tar.gz" reads true and unpacks under "backup"; "notes.txt" reads false.
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_lowercase();
    EXTENSIONS.iter().chain(SEVEN_TAILS.iter()).any(|ext| lower.len() > ext.len() && lower.ends_with(ext))
}

// Sample input: "photos.zip" unpacks into "photos"; "plain" is its own directory name.
pub fn extract_dir(name: &str) -> String {
    let lower = name.to_lowercase();
    for ext in EXTENSIONS.iter().chain(SEVEN_TAILS.iter()) {
        if lower.len() > ext.len() && lower.ends_with(ext) {
            if let Some(cut) = matched_cut(name, ext) {
                return name[..cut].to_string();
            }
        }
    }
    name.to_string()
}

// The cut in the original name, since a folded char (U+212A reads "k") can differ in length from its lowercase.
fn matched_cut(name: &str, ext: &str) -> Option<usize> {
    for (at, _) in name.char_indices().rev() {
        let tail = name[at..].to_lowercase();
        if tail == ext {
            return (at > 0).then_some(at);
        }
        if tail.len() > ext.len() {
            return None;
        }
    }
    None
}

// Sample input: one row compresses under "notes", several under the directory holding them.
pub fn archive_stem(names: &[&str], parent_leaf: &str) -> String {
    if names.len() == 1 {
        return strip_extension(names[0]);
    }
    if parent_leaf.is_empty() { "archive".into() } else { parent_leaf.into() }
}

pub fn strip_extension(name: &str) -> String {
    match name.rfind('.') {
        Some(cut) if cut > 0 => name[..cut].to_string(),
        _ => name.to_string(),
    }
}

fn has_suffix(lower: &str, suffix: &str) -> bool {
    lower.len() >= suffix.len() && lower.ends_with(suffix)
}

// Sample input: an ".iso" lists only, so it offers no extract even with every tool installed.
pub fn is_list_only(name: &str) -> bool {
    let lower = name.to_lowercase();
    LIST_ONLY.iter().any(|only| has_suffix(&lower, only))
}

/// The backend's probed capabilities: 7z, the zip class and plain tar each read their own class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub seven_zip: bool,
    pub zip: bool,
    pub archive: bool,
}

// Sample input: a box with only 7z reads the zip class but no tar.
pub fn can_extract(name: &str, caps: Caps) -> bool {
    let lower = name.to_lowercase();
    if is_list_only(name) {
        return false;
    }
    if has_suffix(&lower, ".7z") {
        return caps.seven_zip;
    }
    if has_suffix(&lower, ".zip") || has_suffix(&lower, ".rar") {
        return caps.zip;
    }
    if SEVEN_TAILS.iter().any(|tail| has_suffix(&lower, tail)) {
        return caps.seven_zip;
    }
    caps.archive
}

// Sample input: "7z" is offered first even when the backend reports "zip" first.
pub fn board_formats(reported: &[&str]) -> Vec<String> {
    let offered: Vec<String> =
        BOARD_ORDER.iter().filter(|f| reported.contains(f)).map(|f| (*f).to_string()).collect();
    if offered.is_empty() { vec!["7z".into()] } else { offered }
}

// The backend's two password codes, matched exactly the way "cancelled" routes, never as sentences.
pub fn needs_password(err: &str) -> bool {
    err == "encrypted"
}

// Sample input: "wrong-password" reopens the card; "the archive tool failed" only messages.
pub fn wrong_password(err: &str) -> bool {
    err == "wrong-password"
}

/// The backend-stopped sentence an archive names, or the plain one with no archive in hand.
pub fn backend_stopped_line(i18n: &crate::i18n::I18n, name: Option<&str>) -> String {
    match name {
        Some(name) => i18n.tr_args("archive.backend_stopped", &[name]),
        None => i18n.tr("archive.backend_stopped_plain"),
    }
}

/// What an extract's terminal line does: "password" opens the card, "wrong" reopens it with its sentence.
pub fn done_action(ok: bool, err: &str) -> &'static str {
    if !ok && needs_password(err) {
        return "password";
    }
    if !ok && wrong_password(err) {
        return "wrong";
    }
    ""
}
