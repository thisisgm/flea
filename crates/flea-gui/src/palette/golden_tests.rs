use super::Theme;
use gpui::{Hsla, Rgba};
use std::collections::{BTreeMap, BTreeSet};

type Cases = BTreeMap<(String, u32), BTreeMap<String, String>>;

// The same table, read the same way, as the theme core's own golden test.
fn cases() -> Cases {
    let mut out = Cases::new();
    for line in include_str!("../../../../tests/fixtures/theme-golden.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "golden line {line}");
        let key = (fields[0].to_owned(), fields[1].parse().expect("size stop"));
        let prior = out
            .entry(key)
            .or_default()
            .insert(fields[2].into(), fields[3].into());
        assert!(prior.is_none(), "duplicate golden line {line}");
    }
    out
}

fn unnumber(key: &str) -> &str {
    match key.split_once('.') {
        Some((head, rest)) if head.len() == 6 && head.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => key,
    }
}

fn colors_text(rows: &BTreeMap<String, String>) -> String {
    let mut colors = String::new();
    for (key, value) in rows {
        if let Some(key) = key.strip_prefix("input.colors.") {
            colors.push_str(&format!("{} = \"{value}\"\n", unnumber(key)));
        }
    }
    colors
}

fn argb(color: Hsla) -> String {
    let c: Rgba = color.into();
    let q = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", q(c.a), q(c.r), q(c.g), q(c.b))
}

// Every role the composer's theme carries, keyed by the ThemeColors token it must equal.
fn roles(theme: &Theme) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        ("color.background", argb(theme.background)),
        ("color.foreground", argb(theme.foreground)),
        ("color.surface", argb(theme.surface)),
        ("color.accent", argb(theme.accent)),
        ("color.accentFrame", argb(theme.accent_frame)),
        ("color.metadata", argb(theme.metadata)),
        ("color.metadataFrame", argb(theme.metadata_frame)),
        ("color.mutedFrame", argb(theme.muted_frame)),
        ("color.symlink", argb(theme.symlink)),
        ("color.executable", argb(theme.executable)),
    ])
}

#[test]
fn m9_integ_g05_palette_equals_the_golden_tokens_for_every_stock_palette() {
    let cases = cases();
    let names: BTreeSet<_> = cases.keys().map(|(name, _)| name.clone()).collect();
    assert!(names.len() >= 23, "stock palette coverage shrank");
    for ((name, stop), rows) in &cases {
        let (theme, parsed) = Theme::from_text_observed(&colors_text(rows));
        assert!(parsed.0, "{name}/{stop}: the palette did not parse");
        for (token, value) in roles(&theme) {
            assert_eq!(Some(&value), rows.get(token), "{name}/{stop}/{token}");
        }
    }
}

// v0.3.12 Row.qml:425-432 and SidebarRow.qml:229 draw links, executables and mounted volumes in their own inks.
#[test]
fn m9_integ_g05_row_inks_are_the_theme_roles_not_the_foreground() {
    let theme = Theme::from_text(
        "background='#101315'\nforeground='#cacccc'\naccent='#e5a55b'\ncyan='#56b6c2'\ngreen='#98c379'",
    );
    let palette = crate::list::Palette::inherited(&gpui::TextStyle::default(), theme);
    assert_eq!(palette.symlink, theme.symlink);
    assert_eq!(palette.executable, theme.executable);
    assert_eq!(palette.metadata, theme.metadata);
    assert_ne!(palette.symlink, palette.foreground);
    assert_ne!(palette.executable, palette.foreground);
    assert_ne!(palette.symlink, palette.executable);
}

// EY-298 (c): ColumnRow.qml:79 rests an ancestor column's names in Theme.color.muted, as the columns board draws them.
#[test]
fn ey298_a_dimmed_columns_name_takes_the_muted_role_in_every_stock_palette() {
    use crate::columns::paint::{Clip, Palette};
    for ((name, stop), rows) in &cases() {
        let theme = Theme::from_text(&colors_text(rows));
        let palette = Palette::inherited(&gpui::TextStyle::default(), theme);
        let dim = palette.row_ink(Clip::None, false, false, true).name;
        assert_eq!(Some(&argb(dim)), rows.get("color.muted"), "{name}/{stop}");
        assert_eq!(
            palette.row_ink(Clip::None, false, true, true).name,
            palette.foreground,
            "{name}/{stop}: the open folder lifts to foreground"
        );
    }
}
