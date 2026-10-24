// Shared visibility, menu wording and output names for the window and terminal clients.
use super::{aliases::Aliases, mime::Db, officepdf};
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

pub use flea::ui_logic_op4::output::*;

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

#[cfg(test)]
pub fn print_label(count: usize) -> String {
    if count > 1 { format!("Print {count} Items…") } else { "Print…".into() }
}

pub fn title(verb: &str, paths: &[PathBuf]) -> String {
    if paths.len() == 1 { format!("{verb} {}", paths[0].file_name().unwrap_or_default().to_string_lossy()) }
    else { format!("{verb} {} Items", paths.len()) }
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
