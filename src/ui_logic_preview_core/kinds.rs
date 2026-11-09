// How a row is classified for a preview: its state and the extension families the backend's icon cannot tell apart.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    Code,
    Archive,
    Symlink,
    Multi,
    Loading,
    Error,
    Unsupported,
    Font,
}

// src/backend/rows.rs UNKNOWN_KIND: the Kind a row gets when nothing could describe its type.
pub const UNKNOWN_KIND: &str = "Data";

const CODE_EXTENSIONS: [&str; 38] = [
    ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".c", ".h", ".cpp", ".hpp", ".cc", ".go", ".rb", ".java", ".kt", ".swift",
    ".lua", ".pl", ".php", ".sh", ".bash", ".zsh", ".fish", ".vim", ".qml", ".css", ".scss", ".html", ".xml", ".json",
    ".toml", ".yaml", ".yml", ".ini", ".conf", ".sql", ".mjs", ".cjs",
];
const MARKDOWN_EXTENSIONS: [&str; 3] = [".md", ".markdown", ".mkd"];
const FONT_EXTENSIONS: [&str; 3] = [".ttf", ".otf", ".ttc"];
const RAW_EXTENSIONS: [&str; 11] = [".cr2", ".cr3", ".nef", ".nrw", ".arw", ".sr2", ".dng", ".raf", ".orf", ".rw2", ".pef"];
const MODEL_EXTENSIONS: [&str; 6] = [".stl", ".3mf", ".obj", ".mtl", ".gltf", ".glb"];
const TABLE_EXTENSIONS: [&str; 2] = [".csv", ".tsv"];
const HTML_EXTENSIONS: [&str; 2] = [".html", ".htm"];
// ui/js/Archive.js EXTENSIONS, the 7z-read tail included.
pub const ARCHIVE_EXTENSIONS: [&str; 17] = [
    ".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar", ".zip", ".7z", ".rar", ".appimage", ".cpio", ".cab",
    ".jar", ".apk", ".rpm", ".deb", ".iso",
];

// A strict suffix, case-insensitive: "a.rs" ends in ".rs" and "rs" alone does not.
fn ends_with_any(name: &str, extensions: &[&str]) -> bool {
    let lower = name.to_lowercase();
    extensions.iter().any(|e| lower.len() > e.len() && lower.ends_with(e))
}

// A suffix that may be the whole name: ui/js/Kinds.js tests these with an anchored regular expression.
fn ends_with_word(name: &str, extensions: &[&str]) -> bool {
    let lower = name.to_lowercase();
    extensions.iter().any(|e| lower.ends_with(e))
}

pub fn is_code(name: &str) -> bool { ends_with_any(name, &CODE_EXTENSIONS) }
pub fn is_markdown(name: &str) -> bool { ends_with_any(name, &MARKDOWN_EXTENSIONS) }
pub fn is_html(name: &str) -> bool { ends_with_word(name, &HTML_EXTENSIONS) }
pub fn is_font(name: &str) -> bool { ends_with_any(name, &FONT_EXTENSIONS) }
pub fn is_raw(name: &str) -> bool { ends_with_word(name, &RAW_EXTENSIONS) }
pub fn is_model(name: &str) -> bool { ends_with_word(name, &MODEL_EXTENSIONS) }
pub fn is_archive(name: &str) -> bool { ends_with_any(name, &ARCHIVE_EXTENSIONS) }
fn is_table(name: &str) -> bool { ends_with_word(name, &TABLE_EXTENSIONS) }

pub fn is_pdf(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.len() > 4 && lower.ends_with(".pdf")
}

// The icon is the classification the backend already made; its freedesktop family classifies.
pub fn kind_state(icon: &str) -> State {
    match icon {
        "text-x-script" | "text-html" | "application-xml" => return State::Code,
        "package-x-generic" => return State::Archive,
        _ => {}
    }
    if icon.starts_with("image-") {
        State::Image
    } else if icon.starts_with("video-") {
        State::Video
    } else if icon.starts_with("audio-") {
        State::Audio
    } else if icon.starts_with("text-") {
        State::Text
    } else {
        // An office document shares its icon with a PDF; what is left is binary.
        State::Unsupported
    }
}

// A name the icon cannot route, told apart by its own extension.
pub fn state_by_extension(name: &str) -> Option<State> {
    if is_pdf(name) {
        Some(State::Pdf)
    } else if is_archive(name) {
        Some(State::Archive)
    } else if is_table(name) {
        Some(State::Text)
    } else {
        None
    }
}

// Quick Look's kind from the icon, the name and a served thumbnail; code and Markdown read as text.
pub fn quick_look_kind(icon: &str, name: &str, thumbnailed: bool) -> State {
    if is_pdf(name) {
        return State::Pdf;
    }
    if is_font(name) {
        return State::Font;
    }
    if is_markdown(name) || is_table(name) {
        return State::Text;
    }
    if is_archive(name) {
        return State::Archive;
    }
    match kind_state(icon) {
        State::Image if is_model(name) && !thumbnailed => State::Unsupported,
        State::Code => State::Text,
        other => other,
    }
}

// Which content unit Quick Look opens a row with, the one ui/QuickLookWarm.qml held compiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Markdown,
    Pdf,
    Media,
    Image,
    Text,
}

pub fn unit_of(name: &str, icon: &str, is_dir: bool, thumbnailed: bool) -> Option<Unit> {
    if is_dir {
        return None;
    }
    if is_markdown(name) {
        return Some(Unit::Markdown);
    }
    match quick_look_kind(icon, name, thumbnailed) {
        State::Pdf => Some(Unit::Pdf),
        State::Video | State::Audio => Some(Unit::Media),
        State::Image => Some(Unit::Image),
        State::Text => Some(Unit::Text),
        _ => None,
    }
}

// Why Quick Look draws no preview, as Preview.qml and PreviewText.qml pick it: too large first, then a failed read, then no kind.
pub fn cannot_show_key(state: State, too_large: bool, failed: bool) -> Option<&'static str> {
    if too_large {
        Some("preview.too_large")
    } else if failed || state == State::Error {
        Some("preview.unreadable")
    } else if state == State::Unsupported {
        Some("preview.unsupported")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_preview_names_its_reason() {
        assert_eq!(cannot_show_key(State::Text, true, false), Some("preview.too_large"));
        // too large outranks a failed read, as PreviewText's status does
        assert_eq!(cannot_show_key(State::Text, true, true), Some("preview.too_large"));
        assert_eq!(cannot_show_key(State::Pdf, false, true), Some("preview.unreadable"));
        assert_eq!(cannot_show_key(State::Error, false, false), Some("preview.unreadable"));
        assert_eq!(cannot_show_key(State::Unsupported, false, false), Some("preview.unsupported"));
        // a kind that previews has no sentence
        assert_eq!(cannot_show_key(State::Image, false, false), None);
        let i18n = crate::i18n::test_catalogs::english();
        assert_eq!(i18n.tr("preview.too_large"), "This file is too large to preview.");
        assert_eq!(i18n.tr("preview.unreadable"), "This file could not be read.");
        assert_eq!(i18n.tr("preview.unsupported"), "This file cannot be previewed.");
    }

    #[test]
    fn quick_look_units_follow_quicklookkinds_js() {
        assert_eq!(unit_of("a.pdf", "x-office-document", false, false), Some(Unit::Pdf));
        assert_eq!(unit_of("a.mp4", "video-x-generic", false, false), Some(Unit::Media));
        assert_eq!(unit_of("a.png", "image-x-generic", false, false), Some(Unit::Image));
        assert_eq!(unit_of("a.md", "text-x-generic", false, false), Some(Unit::Markdown));
        assert_eq!(unit_of("dir", "folder", true, false), None);
        assert_eq!(unit_of("a.tar", "package-x-generic", false, false), None);
        assert_eq!(unit_of("core.bin", "application-x-generic", false, false), None);
    }
}
