// Shared eligibility and output names, used by the backend and both frontends.
use std::path::Path;

pub const IMAGE_FORMATS: &[&str] = &["jpg", "png", "webp", "avif", "heic", "tiff", "bmp", "pdf"];
pub const PDF_FORMATS: &[&str] = &["png", "jpg", "webp", "tiff"];
pub const COMBINE_FORMATS: &[&str] = &["png", "jpg", "webp", "pdf"];
pub const MAX_ITEMS: usize = 256;
pub const MIN_ITEMS: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Pdf,
    Office,
    Text,
    Other,
}

impl Kind {
    pub fn printable(self) -> bool {
        self != Self::Other
    }
    pub fn paged(self) -> bool {
        matches!(self, Self::Pdf | Self::Office)
    }
    pub fn formats(self) -> &'static [&'static str] {
        match self {
            Self::Image => IMAGE_FORMATS,
            Self::Pdf => PDF_FORMATS,
            Self::Office => &["pdf"],
            _ => &[],
        }
    }
}

pub fn can_print(kinds: &[Kind]) -> bool {
    !kinds.is_empty() && kinds.iter().all(|kind| kind.printable())
}

pub fn can_convert(kinds: &[Kind]) -> bool {
    matches!(kinds, [Kind::Pdf | Kind::Office])
        || (!kinds.is_empty() && kinds.iter().all(|kind| *kind == Kind::Image))
}

pub fn can_combine(kinds: &[Kind]) -> bool {
    kinds.len() >= 2 && kinds.iter().all(|kind| *kind == Kind::Image)
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}

// magick expands %d and %x anywhere in an output path (scan%d/o.jpg writes scan0/o.jpg), so each % is doubled in the path's own bytes.
pub fn magick_output(path: &Path) -> String {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let mut doubled = Vec::with_capacity(path.as_os_str().len());
    for &byte in path.as_os_str().as_bytes() {
        doubled.push(byte);
        if byte == b'%' {
            doubled.push(b'%');
        }
    }
    let doubled = std::path::PathBuf::from(std::ffi::OsString::from_vec(doubled));
    crate::bytename::path_text(&doubled).into_owned()
}

pub fn converted_name(source: &Path, format: &str) -> String {
    let word = if extension(source) == format {
        "stripped"
    } else {
        "converted"
    };
    format!(
        "{} ({word}).{format}",
        source.file_stem().unwrap_or_default().to_string_lossy()
    )
}

pub fn page_name(source: &Path, page: u32, format: &str) -> String {
    format!(
        "{} (page {page}).{format}",
        source.file_stem().unwrap_or_default().to_string_lossy()
    )
}

pub fn combined_format(source: &Path) -> &'static str {
    match extension(source).as_str() {
        "jpg" | "jpeg" => "jpg",
        "webp" => "webp",
        _ => "png",
    }
}

pub fn combined_name(source: &Path, format: &str) -> String {
    format!(
        "{} (combined).{format}",
        source.file_stem().unwrap_or_default().to_string_lossy()
    )
}

pub fn bare_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

pub fn page_fallback(path: &Path) -> String {
    format!(
        "{} (converted).{}",
        path.file_stem().unwrap_or_default().to_string_lossy(),
        extension(path)
    )
}

// The most copy names a taken output steps through, as Duplicate does.
pub const COPY_TRIES: usize = 999;

// Duplicate's copy names in the name's own bytes: n 0 is "a.jpg" itself, 1 "a copy.jpg", 2 "a copy 2.jpg".
pub fn copy_path(output: &Path, n: usize) -> Option<std::path::PathBuf> {
    if n == 0 {
        return Some(output.to_path_buf());
    }
    let name = Path::new(output.file_name()?);
    let mut copy = name.file_stem()?.to_os_string();
    copy.push(if n == 1 {
        " copy".to_string()
    } else {
        format!(" copy {n}")
    });
    if let Some(ext) = name.extension() {
        copy.push(".");
        copy.push(ext);
    }
    Some(output.with_file_name(copy))
}
