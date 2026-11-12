//! The palette a desktop hands its GTK 4 apps, for a session without an Omarchy theme.
//!
//! GTK 4 apps read `@define-color` lines from `~/.config/gtk-4.0`, so that folder is where theme
//! generators that want every app to match write their colours: libadwaita names (`window_bg_color`,
//! `accent_bg_color`), the older `theme_*` names, KDE's Breeze export with a `_breeze` suffix, and
//! DankMaterialShell's wallpaper palette in `dank-colors.css`. The answer is colours.toml text, so the
//! GUI and the TUI keep one parser, `crate::colours::entries`.

use std::collections::HashMap;

/// For each colours.toml key Flea reads, the GTK names that fill it, preferred first.
const ROLES: [(&str, &[&str]); 5] = [
    ("background", &["window_bg_color", "theme_bg_color"]),
    ("foreground", &["window_fg_color", "theme_fg_color"]),
    ("accent", &["accent_bg_color", "accent_color", "theme_selected_bg_color"]),
    ("dark_background", &["sidebar_bg_color", "view_bg_color", "theme_base_color"]),
    ("red", &["error_color", "destructive_bg_color", "error_bg_color"]),
];

// `@a` naming `@b` naming `@a` must end, and real themes chain two or three names at most.
const MAX_HOPS: usize = 8;

// Sample input: "@define-color accent_color #c4c0ff;" or "@define-color x @accent_bg_color;".
fn defines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split(';').filter_map(|chunk| {
        let (_, rest) = chunk.rsplit_once("@define-color")?;
        let mut words = rest.split_whitespace();
        let name = words.next()?;
        let value = words.next()?;
        // A value with a space, such as rgba(0, 0, 0, 0.5) or mix(...), is not a plain colour.
        if words.next().is_some() {
            return None;
        }
        Some((name.strip_suffix("_breeze").unwrap_or(name), value))
    })
}

// "#abc", "#aabbcc" and "#aabbccdd" give "#aabbcc"; anything else gives None.
fn hex(value: &str) -> Option<String> {
    let digits = value.strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    match digits.len() {
        3 => Some(digits.chars().flat_map(|c| [c, c]).collect()),
        6 | 8 => Some(digits[..6].to_owned()),
        _ => None,
    }
    .map(|six| format!("#{}", six.to_ascii_lowercase()))
}

/// The colours.toml lines for these stylesheets, a later sheet overriding an earlier one name by
/// name; empty when none of them names a colour Flea uses.
pub fn colours_text(sheets: &[&str]) -> String {
    let mut named: HashMap<&str, &str> = HashMap::new();
    for sheet in sheets {
        named.extend(defines(sheet));
    }
    let resolve = |name: &str| {
        let mut value = *named.get(name)?;
        for _ in 0..MAX_HOPS {
            match value.strip_prefix('@') {
                Some(next) => value = named.get(next.strip_suffix("_breeze").unwrap_or(next))?,
                None => return hex(value),
            }
        }
        None
    };
    ROLES
        .iter()
        .filter_map(|(key, names)| {
            let colour = names.iter().find_map(|name| resolve(name))?;
            Some(format!("{key} = '{colour}'\n"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::colours_text;

    #[test]
    fn libadwaita_names_fill_every_key_and_a_reference_resolves() {
        let sheet = "/* Generated with Matugen */\n\
            @define-color accent_color #c4c0ff;\n\
            @define-color accent_bg_color #C4C0FF;\n\
            @define-color window_bg_color #201f25;\n\
            @define-color window_fg_color #e5e1e9;\n\
            @define-color view_bg_color #1c1b21;\n\
            @define-color sidebar_bg_color @view_bg_color;\n\
            @define-color error_bg_color #f2b8b5;";
        assert_eq!(
            colours_text(&[sheet]),
            "background = '#201f25'\nforeground = '#e5e1e9'\naccent = '#c4c0ff'\n\
             dark_background = '#1c1b21'\nred = '#f2b8b5'\n"
        );
    }

    #[test]
    fn breeze_export_is_read_and_a_later_sheet_wins() {
        let breeze = "@define-color theme_bg_color_breeze #202326;\n\
            @define-color theme_fg_color_breeze #fcfcfc;\n\
            @define-color theme_selected_bg_color_breeze #3daee9;\n\
            @define-color error_color_breeze #da4453;";
        let dms = "@define-color accent_bg_color #c4c0ff;";
        let text = colours_text(&[breeze, dms]);
        assert!(text.contains("background = '#202326'"));
        assert!(text.contains("foreground = '#fcfcfc'"));
        assert!(text.contains("accent = '#c4c0ff'"));
        assert!(text.contains("red = '#da4453'"));
    }

    #[test]
    fn functions_loops_and_short_values_are_skipped() {
        let sheet = "@define-color window_bg_color rgba(0, 0, 0, 0.5);\n\
            @define-color a @b;\n@define-color b @a;\n@define-color accent_bg_color @a;\n\
            @define-color window_fg_color #fff;\n@define-color view_bg_color #12345;";
        assert_eq!(colours_text(&[sheet]), "foreground = '#ffffff'\n");
        assert_eq!(colours_text(&[]), "");
    }
}
