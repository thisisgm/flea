//! The text size a desktop gives its GTK apps, for a session without Omarchy's shell.toml.
//!
//! `gtk-font-name` in `~/.config/gtk-4.0/settings.ini` is what GTK 4 apps start from, and the
//! desktops that set a font (DankMaterialShell, KDE's GTK export, nwg-look, GNOME's settings daemon
//! mirror) write it there as a Pango description: a family, optional styles, then a size in points,
//! or in pixels with a `px` suffix.

// CSS and GTK both define a point as 1/72 inch and a pixel as 1/96 inch at the default DPI.
const PX_PER_PT: f64 = 96.0 / 72.0;
// A size outside this range is a typo or a joke, never a desktop's text size.
const SANE_PX: std::ops::RangeInclusive<f64> = 6.0..=72.0;

// Sample input: "Noto Sans,  10", "Adwaita Sans 11", "Inter Bold 10.5", "Cantarell 14px" or
// "Inter 14 @wght=450 #tnum=1": Pango puts variations (@) and features (#) after the size.
fn size_px(description: &str) -> Option<u32> {
    let last = description
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|word| !word.is_empty())
        .take_while(|word| !word.starts_with(['@', '#']))
        .last()?;
    let px = match last.strip_suffix("px") {
        Some(px) => px.parse::<f64>().ok()?,
        None => last.parse::<f64>().ok()? * PX_PER_PT,
    };
    SANE_PX.contains(&px).then(|| px.round() as u32)
}

/// The base text size in px that `gtk-font-name` names in a settings.ini, if it names one.
pub fn base_px(settings_ini: &str) -> Option<u32> {
    let mut section = "";
    let mut found = None;
    for line in settings_ini.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name;
        } else if section == "Settings" {
            if let Some(value) = line.strip_prefix("gtk-font-name").and_then(|rest| rest.trim_start().strip_prefix('=')) {
                // GTK keeps the last value of a repeated key.
                found = size_px(value.trim().trim_matches('"')).or(found);
            }
        }
    }
    found
}

/// Omarchy shell.toml text that sets this base size, so the GUI's font reader stays the only one.
pub fn shell_text(settings_ini: &str) -> String {
    base_px(settings_ini).map_or_else(String::new, |px| format!("[font]\nbase-size={px}\n"))
}

#[cfg(test)]
mod tests {
    use super::{base_px, shell_text};

    #[test]
    fn points_become_pixels_and_pixels_stay() {
        let ini = |font: &str| format!("[Settings]\ngtk-cursor-theme-size=24\ngtk-font-name={font}\n");
        assert_eq!(base_px(&ini("Noto Sans,  10")), Some(13));
        assert_eq!(base_px(&ini("Adwaita Sans 11")), Some(15));
        assert_eq!(base_px(&ini("Inter Bold 10.5")), Some(14));
        assert_eq!(base_px(&ini("Cantarell 14px")), Some(14));
        assert_eq!(base_px(&ini("\"Noto Sans 12\"")), Some(16));
        assert_eq!(base_px(&ini("Inter 14 @wght=450")), Some(19));
        assert_eq!(base_px(&ini("Inter 14 @wght=450,opsz=14 #tnum=1")), Some(19));
    }

    #[test]
    fn a_missing_or_silly_size_names_nothing() {
        assert_eq!(base_px("[Settings]\ngtk-font-name=Noto Sans\n"), None);
        assert_eq!(base_px("[Settings]\ngtk-font-name=Noto Sans 400\n"), None);
        assert_eq!(base_px("[Other]\ngtk-font-name=Noto Sans 10\n"), None);
        assert_eq!(base_px(""), None);
        assert_eq!(shell_text(""), "");
        assert_eq!(shell_text("[Settings]\ngtk-font-name = Noto Sans 9"), "[font]\nbase-size=12\n");
    }
}
