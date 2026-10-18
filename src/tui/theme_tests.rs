use super::*;

const BENCH: &str = "background='#1c120d'\ndark_background='#150d0a'\nforeground='#c6b395'\naccent='#d28a41'\nmuted='#68544b'\nred='#d65a4f'";
#[test]
fn t3_palette_precedence_and_board_wash_alphas_are_preserved() {
    let theme = Theme::from_text(BENCH);
    assert_eq!(theme.background, ansi("#1c120d", true));
    assert_eq!(theme.cursor, ansi(&blend("#d28a41", "#1c120d", 0.18), true));
    assert_eq!(theme.selected, ansi(&blend("#c6b395", "#1c120d", 0.18), true));
    assert_eq!(theme.tab, ansi(&blend("#c6b395", "#1c120d", 0.08), true));
    assert_eq!(theme.border, ansi(&blend("#c6b395", "#1c120d", 0.12), false));
    assert_ne!(theme.selected, theme.marked);
    let p = Theme::from_text("color7='#112233'\ncolor0='#010203'\ncolor4='#445566'\naccent='#123456'\nred='#654321'\ncolor1='#abcdef'");
    assert_eq!(p.foreground, ansi("#112233", false));
    assert_eq!(p.background, ansi("#010203", true));
    assert_eq!(p.accent, ansi(&ensure("#123456", "#010203", 3.0), false));
    assert_eq!(p.error, ansi(&contrast("#abcdef", "#010203"), false));
    assert_eq!(Theme::from_text("foreground='oops'").foreground, ansi("#cacccc", false));
    assert_eq!(contrast("#ffffff", "#000000"), "#ffffff");
    assert_eq!(contrast("#707880", "#101315"), "#767e85");
}
#[test]
fn t3_color_fallbacks_emit_only_their_supported_sgr_forms() {
    let low = Theme::with_depth(BENCH, Depth::Sixteen);
    let indexed = Theme::with_depth(BENCH, Depth::Indexed);
    assert!(!low.base().contains(";2;") && !low.base().contains(";5;"));
    assert!(indexed.base().contains(";5;") && !indexed.base().contains(";2;"));
    let row = crate::tui::render::frame_tests::entry("notes.txt", false, 99);
    let output = crate::tui::cells::row(&row, 51, None, false, crate::tui::cells::RowStyle { cursor: true, focused: true, ..Default::default() }, &low, "");
    assert!(output.contains("\x1b[7m"));
    assert!(!crate::tui::render::frame_tests::plain(&output).contains('▎'));
}
#[test]
fn t3_selection_plate_keeps_text_contrast_and_grey_error_roles_fall_back() {
    for (foreground, background, accent) in [("#c6b395", "#1c120d", "#d28a41"), ("#575279", "#faf4ed", "#d7827e"),
        ("#e0def4", "#191724", "#c4a7e7"), ("#ffffff", "#000000", "#ffffff")] {
        let mut alpha = 0.35;
        while alpha > 0.12 && ratio(rgb(foreground), rgb(&blend(foreground, background, alpha))) < 4.5 { alpha -= 0.01; }
        assert!(ratio(rgb(foreground), rgb(&blend(foreground, background, alpha))) >= 4.5);
        assert!(ratio(rgb(foreground), rgb(&blend(accent, background, 0.18))) >= 4.5);
    }
    assert_eq!(Theme::from_text("red='#888888'").error, Theme::from_text("").foreground);
}
#[test]
fn t3_live_theme_reads_only_bounded_regular_files_and_follows_changes() {
    let root = crate::backend::testdir::TestDir::new("tui-t3-theme");
    let path = root.file("colors.toml", BENCH);
    let watch = Watch::start(path.clone());
    assert_eq!(watch.updates.recv_timeout(std::time::Duration::from_secs(2)).unwrap(), BENCH);
    let saved = root.file("colors.next", "background='#ffffff'");
    root.assert_contains(&path);
    std::fs::rename(saved, &path).unwrap();
    assert_eq!(watch.updates.recv_timeout(std::time::Duration::from_secs(2)).unwrap(), "background='#ffffff'");
    assert!(read(root.path()).is_none());
    std::fs::write(&path, "x".repeat(65537)).unwrap();
    assert!(read(&path).is_none());
}
#[test]
fn in_place_rewrite_keeps_previous_palette() {
    let root = crate::backend::testdir::TestDir::new("theme-rewrite");
    let path = root.file("colors.toml", BENCH);
    root.assert_contains(&path);
    let watch = Watch::start(path.clone());
    assert_eq!(watch.updates.recv_timeout(std::time::Duration::from_secs(3)).unwrap(), BENCH);
    let _writer = std::fs::OpenOptions::new().write(true).truncate(true).open(&path).unwrap();
    let transient = watch.updates.recv_timeout(std::time::Duration::from_millis(750)).ok();
    std::fs::write(&path, "background='#ffff").unwrap();
    let partial = watch.updates.recv_timeout(std::time::Duration::from_millis(750)).ok();
    std::fs::write(&path, "background='#ffffff'").unwrap();
    let complete = watch.updates.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
    println!("transient {transient:?}; partial {partial:?}; complete {complete:?}");
    assert_eq!(complete, "background='#ffffff'");
    assert_eq!((transient, partial), (None, None), "a rewrite caught empty or cut short replaced the live palette");
}
