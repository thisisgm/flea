//! Rail glyphs reuse public file and plugin marks; rail-only paths are the shipped Icons.js data.
use gpui::SharedString;
macro_rules! cut {
    ($data:literal) => {
        concat!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#, $data, r#""/></svg>"#)
    };
}

/// Unknown manifest glyphs keep the committed plugin registry's refusal.
pub fn svg(name: &str) -> Option<(SharedString, &'static [u8])> {
    let (key, data): (&'static str, &'static str) = match name {
        "house" => ("flea-rail:house", cut!("M3 21V10l9-7 9 7v11h-6v-8H9v8H3z")),
        "history" => ("flea-rail:history", cut!("M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5")),
        "trash" => ("flea-rail:trash", cut!("M3 6h18 M8 6V3h8v3 M6 6l1.2 15h9.6L18 6 M10 10v7 M14 10v7")),
        "drive" => ("flea-rail:drive", cut!("M6 4h12l4 8v8H2v-8z M2 12h20 M6 16L6.01 16 M10 16L10.01 16")),
        "folder-git-2" => ("flea-rail:folder-git-2", cut!("M18 19a5 5 0 0 1-5-5v8 M9 20H2V3h6l2 2h12v5 M11 12A2 2 0 1 0 15 12A2 2 0 1 0 11 12Z M18 19A2 2 0 1 0 22 19A2 2 0 1 0 18 19Z")),
        "server" => (
            "flea-rail:server",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M2 2h20v8H2z"/><path d="M2 14h20v8H2z"/><path d="M6 6L6.01 6"/><path d="M6 18L6.01 18"/></svg>"##,
        ),
        "smartphone" => (
            "flea-rail:smartphone",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M5 2h14v20H5z"/><path d="M12 18L12.01 18"/></svg>"##,
        ),
        "camera" => (
            "flea-rail:camera",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M14.5 4h-5L7 7H2v13h20V7h-5z"/><path d="M15 13a3 3 0 1 1-6 0a3 3 0 1 1 6 0"/></svg>"##,
        ),
        "eject" => (
            "flea-rail:eject",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M12 2 22 13H2z"/><path d="M3 17h18v4H3z"/></svg>"##,
        ),
        "file-text" => return Some(crate::row::marks::Mark::FileText.svg()),
        "music" => return Some(crate::row::marks::Mark::Music.svg()),
        _ => return crate::g2::st2::glyphs::glyph(name),
    };
    Some((SharedString::new_static(key), data.as_bytes()))
}
