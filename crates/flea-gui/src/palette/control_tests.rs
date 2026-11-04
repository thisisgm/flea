use super::Theme;

#[test]
fn fix9_control_roles_keep_the_two_oracle_palettes() {
    for (text, accent, surface) in [
        ("background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'", 0xd28a41, 0x150d0a),
        ("background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e6e9ef'", 0x1e66f5, 0xe6e9ef),
    ] {
        let theme = Theme::from_text(text);
        assert_eq!(theme.accent, gpui::rgb(accent).into());
        assert_eq!(theme.accent_frame, gpui::rgb(accent).into());
        assert_eq!(theme.surface, gpui::rgb(surface).into());
    }
}

fn assert_accent_roles(case: &str, text: &str, foreground: u32, accent: u32, surface: u32) {
    let theme = Theme::from_text(text);
    assert_eq!(
        theme.foreground,
        gpui::rgb(foreground).into(),
        "foreground: {case}"
    );
    assert_eq!(
        theme.accent,
        gpui::rgb(accent).into(),
        "accent precedence: {case}"
    );
    assert_eq!(
        theme.surface,
        gpui::rgb(surface).into(),
        "surface precedence: {case}"
    );
    let rgb =
        |color| flea::ui_logic_pl4::tagtheme::parse(&crate::ipc::rows::colour(color)).unwrap();
    let ratio = flea::ui_logic_pl4::tagtheme::ratio_of;
    assert!(
        ratio(rgb(theme.accent_frame), rgb(theme.surface)) >= 3.,
        "frame contrast on surface: {case}"
    );
    if ratio(rgb(theme.accent), rgb(theme.surface)) >= 3. {
        assert_eq!(
            theme.accent_frame, theme.accent,
            "already legible frame: {case}"
        );
    } else {
        assert_ne!(theme.accent_frame, theme.accent, "raised frame: {case}");
    }
}

#[test]
fn accent_precedence_explicit_wins_blue_and_color4() {
    assert_accent_roles(
        "explicit accent",
        "background='#101315'\nforeground='#cacccc'\naccent='#e5a55b'\nblue='#123456'\ncolor4='#abcdef'\ndark_background='#181825'\nselection='#ffffff'",
        0xcacccc,
        0xe5a55b,
        0x181825,
    );
}

#[test]
fn accent_precedence_blue_wins_color4() {
    assert_accent_roles(
        "blue precedence",
        "background='#eff1f5'\nforeground='#4c4f69'\nblue='#1e66f5'\ncolor4='#abcdef'\nselection='#101315'",
        0x4c4f69,
        0x1e66f5,
        0xeff1f5,
    );
}

#[test]
fn accent_precedence_color4_wins_foreground() {
    assert_accent_roles(
        "color4 precedence",
        "foreground='#abcdef'\ncolor4='#123456'\nselection='#101315'",
        0xabcdef,
        0x123456,
        0x101315,
    );
}

#[test]
fn accent_precedence_custom_foreground_fallback() {
    assert_accent_roles(
        "custom foreground fallback",
        "background='#eff1f5'\nforeground='#112233'\ncolor7='#abcdef'",
        0x112233,
        0x112233,
        0xeff1f5,
    );
}

#[test]
fn accent_precedence_color7_foreground_fallback() {
    assert_accent_roles(
        "color7 foreground fallback",
        "color7='#345678'",
        0x345678,
        0x345678,
        0x181825,
    );
}
