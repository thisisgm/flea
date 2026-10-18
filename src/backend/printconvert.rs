// Shared visibility, menu wording and output names for the window and terminal clients.
use super::{aliases::Aliases, mime::Db, officepdf};
use std::path::{Path, PathBuf};

pub const IMAGE_FORMATS: &[&str] = &["jpg", "png", "webp", "avif", "heic", "tiff", "bmp", "pdf"];
pub const PDF_FORMATS: &[&str] = &["png", "jpg", "webp", "tiff"];
pub const COMBINE_FORMATS: &[&str] = &["png", "jpg", "webp"];
pub const MAX_ITEMS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Image, Pdf, Office, Text, Other }

impl Kind {
    pub fn printable(self) -> bool { self != Self::Other }
    pub fn paged(self) -> bool { matches!(self, Self::Pdf | Self::Office) }
    pub fn formats(self) -> &'static [&'static str] {
        match self { Self::Image => IMAGE_FORMATS, Self::Pdf => PDF_FORMATS, Self::Office => &["pdf"], _ => &[] }
    }
}

pub fn classify(path: &str, mime: &Db, aliases: &Aliases) -> Kind {
    if officepdf::is_office(path, mime, aliases) { return Kind::Office; }
    let Some(mime) = mime.lookup(path) else { return Kind::Other; };
    let mime = aliases.canonical(mime);
    if mime == "application/pdf" { Kind::Pdf }
    else if mime.starts_with("image/") { Kind::Image }
    else if mime.starts_with("text/") || matches!(mime, "application/json" | "application/xml" | "application/javascript"
        | "application/x-shellscript" | "application/x-python" | "application/x-perl" | "application/x-ruby"
        | "application/x-php" | "application/sql" | "application/toml" | "application/yaml") { Kind::Text }
    else { Kind::Other }
}

pub fn can_print(kinds: &[Kind]) -> bool {
    !kinds.is_empty() && kinds.iter().all(|kind| kind.printable())
}

pub fn can_convert(kinds: &[Kind]) -> bool {
    matches!(kinds, [Kind::Pdf | Kind::Office]) || (!kinds.is_empty() && kinds.iter().all(|kind| *kind == Kind::Image))
}

pub fn can_combine(kinds: &[Kind]) -> bool {
    kinds.len() >= 2 && kinds.iter().all(|kind| *kind == Kind::Image)
}

pub fn print_label(count: usize) -> String {
    if count > 1 { format!("Print {count} Items…") } else { "Print…".into() }
}

pub fn title(verb: &str, paths: &[PathBuf]) -> String {
    if paths.len() == 1 { format!("{verb} {}", paths[0].file_name().unwrap_or_default().to_string_lossy()) }
    else { format!("{verb} {} Items", paths.len()) }
}

pub fn extension(path: &Path) -> String {
    path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase()
}

pub fn converted_name(source: &Path, format: &str) -> String {
    let word = if extension(source) == format { "stripped" } else { "converted" };
    format!("{} ({word}).{format}", source.file_stem().unwrap_or_default().to_string_lossy())
}

pub fn page_name(source: &Path, page: u32, format: &str) -> String {
    format!("{} (page {page}).{format}", source.file_stem().unwrap_or_default().to_string_lossy())
}

pub fn combined_format(source: &Path) -> &'static str {
    match extension(source).as_str() { "jpg" | "jpeg" => "jpg", "webp" => "webp", _ => "png" }
}

pub fn combined_name(source: &Path, format: &str) -> String {
    format!("{} (combined).{format}", source.file_stem().unwrap_or_default().to_string_lossy())
}

pub fn bare_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_require_every_selected_kind_to_support_the_action() {
        use Kind::*;
        assert_eq!(print_label(3), "Print 3 Items…");
        assert_eq!(print_label(1), "Print…");
        assert!(can_print(&[Pdf, Office, Text, Image]));
        assert!(!can_print(&[Image, Other]));
        assert!(!can_print(&[]));
        assert!(can_convert(&[Image, Image]));
        assert!(!can_convert(&[Pdf, Image]));
        assert!(!can_convert(&[Office, Office]));
        assert!(can_combine(&[Image, Image]));
        assert!(!can_combine(&[Image]));
        assert!(!can_combine(&[Image, Pdf]));
    }

    #[test]
    fn names_match_the_shipped_converter_and_the_new_boards() {
        let source = Path::new("/photos/photo.png");
        assert_eq!(converted_name(source, "pdf"), "photo (converted).pdf");
        assert_eq!(converted_name(source, "png"), "photo (stripped).png");
        assert_eq!(page_name(Path::new("report.pdf"), 5, "png"), "report (page 5).png");
        assert_eq!(combined_name(source, "png"), "photo (combined).png");
        assert_eq!(combined_format(Path::new("photo.JPEG")), "jpg");
        assert_eq!(combined_format(Path::new("photo.tiff")), "png");
        assert_eq!(title("Print", &[PathBuf::from("/x/report.pdf")]), "Print report.pdf");
        assert_eq!(title("Convert", &[PathBuf::from("a"), PathBuf::from("b")]), "Convert 2 Items");
        for name in ["", ".", "..", "../escape.png", "x\0.png"] { assert!(!bare_name(name)); }
    }
}
