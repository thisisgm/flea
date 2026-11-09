//! Menu marks: the Lucide 1.38.0 cut ui/js/Icons.js carries (ISC), the boards' printer and images, and the three brand marks.
use flea::ui_logic_mn1::entry::MARK_NAMES;
use flea::ui_logic_mn1::Brand;
use gpui::SharedString;

macro_rules! cut {
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

// Every mark a menu row, flyout leaf or chevron names, by its Icons.js name, in MARK_NAMES order (pinned by a test).
pub(super) const CUT: &[(&str, &str, &[u8])] = &[
    ("app-window", "flea-menu:app-window", cut!("M3 4h18v16H3z M3 9h18 M6 6.5h.01 M9 6.5h.01")),
    ("archive", "flea-menu:archive", cut!("M2 3h20v5H2z M4 8v13h16V8 M10 12h4")),
    ("archive-out", "flea-menu:archive-out", cut!("M2 3h20v5H2z M4 8v13h16V8 M12 18v-6 M9 15l3-3 3 3")),
    ("check", "flea-menu:check", cut!("M4 12l6 6L20 6")),
    // The menus' own checkmark, the board's cut for the order or state in effect.
    ("checkmark", "flea-menu:checkmark", cut!("M5 12l5 5 9-10")),
    ("chevron-right", "flea-menu:chevron-right", cut!("M9 18l6-6-6-6")),
    ("clipboard", "flea-menu:clipboard", cut!("M9 2h6v4H9z M6 4H3v18h18V4h-3 M8 12h8 M8 16h5")),
    ("code", "flea-menu:code", cut!("M16 18l6-6-6-6 M8 6l-6 6 6 6")),
    ("columns", "flea-menu:columns", cut!("M3 3h18v18H3z M9 3v18 M15 3v18")),
    ("contrast", "flea-menu:contrast", cut!("M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0-20 M12 18a6 6 0 0 0 0-12v12z")),
    ("copy", "flea-menu:copy", cut!("M9 8h12v13H9z M4 16V3h13")),
    ("download", "flea-menu:download", cut!("M12 15V3 M7 10l5 5 5-5 M3 15v6h18v-6")),
    ("drive", "flea-menu:drive", cut!("M6 4h12l4 8v8H2v-8z M2 12h20 M6 16L6.01 16 M10 16L10.01 16")),
    ("eject", "flea-menu:eject", cut!("M12 2 22 13H2z M3 17h18v4H3z")),
    ("eye", "flea-menu:eye", cut!("M12 5 22 12 12 19 2 12z M10 10h4v4h-4z")),
    ("eye-off", "flea-menu:eye-off", cut!("M12 5 22 12 12 19 2 12z M10 10h4v4h-4z M4 4l16 16")),
    ("file", "flea-menu:file", cut!("M4 22V2h10l6 6v14H4z M14 2v6h6")),
    ("file-plus", "flea-menu:file-plus", cut!("M6 22V2h8l4 4v16H6z M14 2v4h4 M12 11v6 M9 14h6")),
    ("file-text", "flea-menu:file-text", cut!("M4 22V2h10l6 6v14H4z M14 2v6h6 M8 9h2 M8 13h8 M8 17h8")),
    ("folder", "flea-menu:folder", cut!("M2 20V3h6l2 3h12v14H2z")),
    ("folder-open", "flea-menu:folder-open", cut!("M2 20V3h6l2 3h12v3 M22 11l-2.5 9H2l2.5-9z")),
    ("folder-plus", "flea-menu:folder-plus", cut!("M2 20V3h6l2 3h12v14H2z M12 10v6 M9 13h6")),
    ("globe", "flea-menu:globe", cut!("M2 12h20 M12 2a15 15 0 0 1 0 20 M12 2a15 15 0 0 0 0 20 M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0")),
    ("image", "flea-menu:image", cut!("M3 3h18v18H3z M7 9A2 2 0 1 0 11 9A2 2 0 1 0 7 9Z M21 15l-3-3L6 21")),
    ("images", "flea-menu:images", cut!("M7 2h15v15H7z M3 6v16h15 M10 7A1.5 1.5 0 1 0 13 7A1.5 1.5 0 1 0 10 7Z M22 12l-3-3-8 8")),
    ("info", "flea-menu:info", cut!("M12 11v6 M12 7h.01 M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0")),
    ("key", "flea-menu:key", cut!("M3 7a4 4 0 1 0 8 0a4 4 0 1 0-8 0 M10.5 10.5l8 8 M16 16l2.5-2.5 M18.5 18.5l1.5-1.5")),
    ("lock", "flea-menu:lock", cut!("M4 11h16v10H4z M8 11V7a4 4 0 0 1 8 0v4 M12 15h.01")),
    ("minus", "flea-menu:minus", cut!("M5 12h14")),
    ("network", "flea-menu:network", cut!("M9 2h6v6H9z M2 16h6v6H2z M16 16h6v6h-6z M12 8v4 M5 16v-4h14v4")),
    ("play", "flea-menu:play", cut!("M6 4l14 8-14 8z")),
    ("plus", "flea-menu:plus", cut!("M5 12h14 M12 5v14")),
    ("printer", "flea-menu:printer", cut!("M6 9V2h12v7 M6 18H2V9h20v9h-4 M6 14h12v8H6z")),
    ("rename", "flea-menu:rename", cut!("M3 21l1-4L17 4l3 3L7 20l-4 1z M14 7l3 3")),
    ("scissors", "flea-menu:scissors", cut!("M5 3l14 14 M19 3L5 17 M2 17h5v5H2z M17 17h5v5h-5z")),
    ("server", "flea-menu:server", cut!("M2 2h20v8H2z M2 14h20v8H2z M6 6L6.01 6 M6 18L6.01 18")),
    ("shelf", "flea-menu:shelf", cut!("M21 21H3V3h18v14H7V7h10v6h-6")),
    ("sliders", "flea-menu:sliders", cut!("M4 21v-7 M4 10V3 M12 21v-9 M12 8V3 M20 21v-5 M20 12V3 M2 14h4 M10 8h4 M18 16h4")),
    ("sort", "flea-menu:sort", cut!("M11 5h10 M11 9h7 M11 13h4 M7 5v14 M7 19l-3-3 M7 19l3-3")),
    ("star", "flea-menu:star", cut!("M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7L12 17.3l-6.2 3.6 1.6-7L2 9.2l7.1-.6z")),
    ("symlink", "flea-menu:symlink", cut!("M7 17L17 7 M8 7h9v9")),
    ("tag", "flea-menu:tag", cut!("M3 3h7l11 11-7 7L3 10z M7.5 7.5L7.51 7.5")),
    ("terminal", "flea-menu:terminal", cut!("M12 19h8 M4 17l6-6-6-6")),
    ("trash", "flea-menu:trash", cut!("M3 6h18 M8 6V3h8v3 M6 6l1.2 15h9.6L18 6 M10 10v7 M14 10v7")),
    ("type", "flea-menu:type", cut!("M12 4v16 M4 7V4h16v3 M9 20h6")),
    ("undo", "flea-menu:undo", cut!("M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5")),
    // Open in Agent and Actions, from the board source (`design-hig/source/ui.py`, lucide's bot and zap cut to the Omarchy edge); the only copy of these cuts; appended so FILE keeps its index.
    ("bot", "flea-menu:bot", cut!("M12 8V4H8 M4 8h16v12H4z M2 14h2 M20 14h2 M15 13v2 M9 13v2")),
    ("zap", "flea-menu:zap", cut!("M13 2 3 14h9l-1 8 10-12h-9l1-8z")),
    // Send with rclone, the rclone-menu board's cloud-upload, the same cut st1 draws; appended so FILE keeps its index.
    ("cloud-upload", "flea-menu:cloud-upload", cut!("M12 13v8 M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242 M8 17l4-4 4 4")),
    ("x", "flea-menu:x", cut!("M6 6l12 12 M18 6 6 18")),
    ("copy-x", "flea-menu:copy-x", cut!("M9 8h12v13H9z M4 16V3h13 M12 11.5l6 6 M18 11.5l-6 6")),
    // AirDrop's devices, board 0401-airdrop-menu's phone and laptop cut to the Omarchy edge; appended so FILE keeps its index.
    ("smartphone", "flea-menu:smartphone", cut!("M5 2h14v20H5z M12 18L12.01 18")),
    ("laptop", "flea-menu:laptop", cut!("M4 5h16v11H4z M2 20h20")),
];

// The cuts mn2 hands the join as data (Verify Checksum's hash), drawn with the same stroke as CUT.
static HANDED: std::sync::LazyLock<Vec<(&'static str, String, String)>> = std::sync::LazyLock::new(
    || {
        flea::ui_logic_mn2::rows::MARKS
        .iter()
        .map(|(id, d)| (*id, format!("flea-menu:{id}"), format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="{d}"/></svg>"#)))
        .collect()
    },
);

/// The submenu chevron a mirrored window draws, pointing left; menus never name it.
pub const CHEVRON_LEFT: (&str, &[u8]) = ("flea-menu:chevron-left", cut!("M15 18l-6-6 6-6"));

/// The atlas key and SVG for a named mark; an unknown name draws the plain file mark, as Icons.pathFor does.
pub fn mark(name: &str) -> (SharedString, &'static [u8]) {
    if let Some((_, key, svg)) = HANDED.iter().find(|(id, _, _)| *id == name) {
        return (key.clone().into(), svg.as_bytes());
    }
    let (_, key, data) = CUT
        .iter()
        .find(|(id, _, _)| *id == name)
        .unwrap_or(&CUT[FILE]);
    (SharedString::new_static(key), data)
}

pub fn known(name: &str) -> bool {
    MARK_NAMES.contains(&name) || HANDED.iter().any(|(id, _, _)| *id == name)
}

const FILE: usize = 16;

// The official artwork reproduced, monochrome in the row's ink: Tailscale's nine dots, Dropbox's five tiles, LocalSend's ring.
pub fn brand(brand: Brand) -> (SharedString, &'static [u8], f32) {
    let (key, data, ratio): (&str, &str, f32) = match brand {
        Brand::Tailscale => ("flea-menu:brand-tailscale", TAILSCALE, 1.),
        Brand::Dropbox => ("flea-menu:brand-dropbox", DROPBOX, 1.18),
        Brand::LocalSend => ("flea-menu:brand-localsend", LOCALSEND, 1.),
        Brand::AirDrop => ("flea-menu:brand-airdrop", AIRDROP, 1.),
    };
    (SharedString::new_static(key), data.as_bytes(), ratio)
}

// Apple's AirDrop mark as board 0401-airdrop-menu draws it: a dot under two open arcs.
const AIRDROP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18" fill="none"><circle cx="9" cy="9" r="1.8" fill="currentColor"/><g stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M 5.74 11.73 A 4.25 4.25 0 1 1 12.26 11.73"/><path d="M 4.74 14.87 A 7.25 7.25 0 1 1 13.26 14.87"/></g></svg>"#;
const TAILSCALE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 169 169" fill="currentColor"><circle cx="30.5" cy="30.5" r="18" opacity="0.4"/><circle cx="84.5" cy="30.5" r="18" opacity="0.4"/><circle cx="138.5" cy="30.5" r="18" opacity="0.4"/><circle cx="30.5" cy="84.5" r="18"/><circle cx="84.5" cy="84.5" r="18"/><circle cx="138.5" cy="84.5" r="18"/><circle cx="30.5" cy="138.5" r="18" opacity="0.4"/><circle cx="84.5" cy="138.5" r="18"/><circle cx="138.5" cy="138.5" r="18" opacity="0.4"/></svg>"#;
const DROPBOX: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 118 100" fill="currentColor"><path d="M29.5 0L59 18.8L29.5 37.6L0 18.8ZM88.5 0L118 18.8L88.5 37.6L59 18.8ZM29.5 37.6L59 56.4L29.5 75.2L0 56.4ZM88.5 37.6L118 56.4L88.5 75.2L59 56.4ZM59 62.4L88.5 81.2L59 100L29.5 81.2Z"/></svg>"#;
const LOCALSEND: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="27.6" fill="currentColor"/><path d="M95.7 40.91A46.6 46.6 0 0 1 95.7 59.09M88.75 75.89A46.6 46.6 0 0 1 75.89 88.75M59.09 95.7A46.6 46.6 0 0 1 40.91 95.7M24.11 88.75A46.6 46.6 0 0 1 11.25 75.89M4.3 59.09A46.6 46.6 0 0 1 4.3 40.91M11.25 24.11A46.6 46.6 0 0 1 24.11 11.25M40.91 4.3A46.6 46.6 0 0 1 59.09 4.3M75.89 11.25A46.6 46.6 0 0 1 88.75 24.11" fill="none" stroke="currentColor" stroke-width="6.9" stroke-linecap="round"/></svg>"#;
