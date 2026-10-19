//! Row marks: Lucide 1.38.0 path data (ISC), the Omarchy cut ui/js/Icons.js carries, drawn as alpha masks.
use gpui::SharedString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Folder,
    File,
    FileText,
    Code,
    Image,
    Film,
    Music,
    Archive,
    Type,
    Terminal,
    Table,
    Presentation,
    Symlink,
}

macro_rules! svg {
    ($d:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
            r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
            $d,
            r#""/></svg>"#
        )
        .as_bytes()
    };
}

impl Mark {
    /// Icons.js glyphForRow(): a symlink by its mode, everything else by its icon name.
    pub fn for_row(icon: &str, mode: u32) -> Self {
        if super::is_symlink(mode) {
            return Self::Symlink;
        }
        match icon {
            "folder" | "inode-directory" => Self::Folder,
            "text-x-generic" | "text-plain" | "x-office-document" => Self::FileText,
            "text-x-script" | "text-html" | "application-xml" => Self::Code,
            "image-x-generic" => Self::Image,
            "video-x-generic" => Self::Film,
            "audio-x-generic" => Self::Music,
            "package-x-generic" => Self::Archive,
            "font-x-generic" => Self::Type,
            "application-x-executable" => Self::Terminal,
            "x-office-spreadsheet" => Self::Table,
            "x-office-presentation" => Self::Presentation,
            _ => Self::File,
        }
    }

    /// The atlas key and the SVG document; the key names the mark so each rasterizes once per size.
    pub fn svg(self) -> (SharedString, &'static [u8]) {
        let (key, data) = match self {
            Self::Folder => ("flea-mark:folder", svg!("M2 20V3h6l2 3h12v14H2z")),
            Self::File => ("flea-mark:file", svg!("M4 22V2h10l6 6v14H4z M14 2v6h6")),
            Self::FileText => (
                "flea-mark:file-text",
                svg!("M4 22V2h10l6 6v14H4z M14 2v6h6 M8 9h2 M8 13h8 M8 17h8"),
            ),
            Self::Code => ("flea-mark:code", svg!("M16 18l6-6-6-6 M8 6l-6 6 6 6")),
            Self::Image => (
                "flea-mark:image",
                svg!("M3 3h18v18H3z M7 9A2 2 0 1 0 11 9A2 2 0 1 0 7 9Z M21 15l-3-3L6 21"),
            ),
            Self::Film => (
                "flea-mark:film",
                svg!("M3 3h18v18H3z M7 3v18 M3 7.5h4 M3 12h18 M3 16.5h4 M17 3v18 M17 7.5h4 M17 16.5h4"),
            ),
            Self::Music => (
                "flea-mark:music",
                svg!("M9 18V5l12-2v13 M3 15h6v6H3z M15 13h6v6h-6z"),
            ),
            Self::Archive => (
                "flea-mark:archive",
                svg!("M2 3h20v5H2z M4 8v13h16V8 M10 12h4"),
            ),
            Self::Type => ("flea-mark:type", svg!("M12 4v16 M4 7V4h16v3 M9 20h6")),
            Self::Terminal => ("flea-mark:terminal", svg!("M12 19h8 M4 17l6-6-6-6")),
            Self::Table => (
                "flea-mark:table",
                svg!("M3 3h18v18H3z M12 3v18 M3 9h18 M3 15h18"),
            ),
            Self::Presentation => (
                "flea-mark:presentation",
                svg!("M2 3h20 M3 3v13h18V3 M7 21l5-5 5 5"),
            ),
            Self::Symlink => ("flea-mark:symlink", svg!("M7 17L17 7 M8 7h9v9")),
        };
        (SharedString::new_static(key), data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_follow_icons_js() {
        assert_eq!(Mark::for_row("folder", 0o120777), Mark::Symlink);
        assert_eq!(Mark::for_row("inode-directory", 0o040755), Mark::Folder);
        assert_eq!(Mark::for_row("text-html", 0o100644), Mark::Code);
        assert_eq!(Mark::for_row("x-office-document", 0o100644), Mark::FileText);
        assert_eq!(Mark::for_row("application-pdf", 0o100644), Mark::File);
        let (key, data) = Mark::Folder.svg();
        assert_eq!(key.as_ref(), "flea-mark:folder");
        let text = std::str::from_utf8(data).unwrap();
        assert!(
            text.starts_with("<svg ")
                && text.ends_with(r#"<path d="M2 20V3h6l2 3h12v14H2z"/></svg>"#)
        );
    }
}
