//! The marks the Plugins boards draw: Icons.js cut glyphs by name and the three brand reproductions.
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

/// The checkbox tick, drawn at 12px with a 3.4 stroke as the boards' .cb does.
pub const TICK: (&str, &[u8]) = (
    "flea-st2:tick",
    concat!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
        r#"stroke-width="3.4" stroke-linecap="square" stroke-linejoin="miter"><path d="M5 12l5 5 9-10"/></svg>"#
    )
    .as_bytes(),
);

const LOCALSEND: &[u8] = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none">"#,
    r#"<g fill="currentColor"><circle cx="12" cy="12" r="6.62"/></g><path d="M 22.97 9.82 A 11.18 11.18 0 0 1 22.97 14.18 M 21.30 18.21 A 11.18 11.18 0 0 1 18.21 21.30 "#,
    r#"M 14.18 22.97 A 11.18 11.18 0 0 1 9.82 22.97 M 5.79 21.30 A 11.18 11.18 0 0 1 2.70 18.21 M 1.03 14.18 A 11.18 11.18 0 0 1 1.03 9.82 "#,
    r#"M 2.70 5.79 A 11.18 11.18 0 0 1 5.79 2.70 M 9.82 1.03 A 11.18 11.18 0 0 1 14.18 1.03 M 18.21 2.70 A 11.18 11.18 0 0 1 21.30 5.79 " "#,
    r#"stroke="currentColor" stroke-width="1.66" fill="none" stroke-linecap="round"/></svg>"#
)
.as_bytes();

const TAILSCALE: &[u8] = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 169 169" fill="currentColor">"#,
    r#"<circle cx="30.5" cy="30.5" r="18" opacity="0.4"/><circle cx="84.5" cy="30.5" r="18" opacity="0.4"/>"#,
    r#"<circle cx="138.5" cy="30.5" r="18" opacity="0.4"/><circle cx="30.5" cy="84.5" r="18"/><circle cx="84.5" cy="84.5" r="18"/>"#,
    r#"<circle cx="138.5" cy="84.5" r="18"/><circle cx="30.5" cy="138.5" r="18" opacity="0.4"/><circle cx="84.5" cy="138.5" r="18"/>"#,
    r#"<circle cx="138.5" cy="138.5" r="18" opacity="0.4"/></svg>"#
)
.as_bytes();

const DROPBOX: &[u8] = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 20.34" fill="currentColor">"#,
    r#"<polygon points="6.00,0.00 12.00,3.82 6.00,7.65 0.00,3.82"/><polygon points="18.00,0.00 24.00,3.82 18.00,7.65 12.00,3.82"/>"#,
    r#"<polygon points="6.00,7.65 12.00,11.47 6.00,15.29 0.00,11.47"/><polygon points="18.00,7.65 24.00,11.47 18.00,15.29 12.00,11.47"/>"#,
    r#"<polygon points="12.00,12.69 18.00,16.52 12.00,20.34 6.00,16.52"/></svg>"#
)
.as_bytes();

// The AirDrop mark board 0401-airdrop-menu draws: a dot under two open arcs.
const AIRDROP: &[u8] = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18" fill="none">"#,
    r#"<circle cx="9" cy="9" r="1.8" fill="currentColor"/><g stroke="currentColor" stroke-width="1.5" stroke-linecap="round">"#,
    r#"<path d="M 5.74 11.73 A 4.25 4.25 0 1 1 12.26 11.73"/><path d="M 4.74 14.87 A 7.25 7.25 0 1 1 13.26 14.87"/></g></svg>"#
)
.as_bytes();

/// Dropbox's own mark is wider than tall, so it sits in its box at the board's 24 by 20.34 aspect.
pub const DROPBOX_ASPECT: f32 = 20.34 / 24.;

/// A glyph by the name a manifest or a row gives; a name Flea does not carry draws no mark.
pub fn glyph(name: &str) -> Option<(SharedString, &'static [u8])> {
    let (key, data): (&'static str, &'static [u8]) = match name {
        "hash" => (
            "flea-st2:hash",
            cut!("M4 9h16 M4 15h16 M10 3 8 21 M16 3l-2 18"),
        ),
        "shelf" => ("flea-st2:shelf", cut!("M21 21H3V3h18v14H7V7h10v6h-6")),
        "localsend" => ("flea-st2:localsend", LOCALSEND),
        "tailscale" => ("flea-st2:tailscale", TAILSCALE),
        "dropbox" => ("flea-st2:dropbox", DROPBOX),
        "airdrop" => ("flea-st2:airdrop", AIRDROP),
        "maximize" => (
            "flea-st2:maximize",
            cut!("M8 3H3v5 M16 3h5v5 M8 21H3v-5 M16 21h5v-5"),
        ),
        "image" => (
            "flea-st2:image",
            cut!("M3 3h18v18H3z M7 9A2 2 0 1 0 11 9A2 2 0 1 0 7 9Z M21 15l-3-3L6 21"),
        ),
        "film" => (
            "flea-st2:film",
            cut!("M3 3h18v18H3z M7 3v18 M3 7.5h4 M3 12h18 M3 16.5h4 M17 3v18 M17 7.5h4 M17 16.5h4"),
        ),
        "list" => (
            "flea-st2:list",
            cut!("M8 6h13 M8 12h13 M8 18h13 M3 6L3.01 6 M3 12L3.01 12 M3 18L3.01 18"),
        ),
        "download" => (
            "flea-st2:download",
            cut!("M12 15V3 M7 10l5 5 5-5 M3 15v6h18v-6"),
        ),
        "plus" => ("flea-st2:plus", cut!("M5 12h14 M12 5v14")),
        "plug" => (
            "flea-st2:plug",
            cut!("M12 22v-5 M9 8V2 M15 8V2 M6 8h12v6l-3 3H9l-3-3z"),
        ),
        "chevron-left" => ("flea-st2:chevron-left", cut!("M15 18l-6-6 6-6")),
        "chevron-right" => ("flea-st2:chevron-right", cut!("M9 18l6-6-6-6")),
        "folder" => ("flea-st2:folder", cut!("M2 20V3h6l2 3h12v14H2z")),
        "file" => ("flea-st2:file", cut!("M4 22V2h10l6 6v14H4z M14 2v6h6")),
        // A menu action's mark (Checksums' hash) names the plugin's row with the menu's own cut.
        menu if crate::g2::mn1::glyphs::known(menu) => {
            return Some(crate::g2::mn1::glyphs::mark(menu))
        }
        _ => return None,
    };
    Some((SharedString::new_static(key), data))
}
