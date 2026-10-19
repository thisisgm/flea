use super::*;

const BENCH: &str = "background='#1c120d'\ndark_background='#150d0a'\nforeground='#c6b395'\naccent='#d28a41'\nmuted='#68544b'\nred='#d65a4f'";
#[test]
fn t3_palette_precedence_and_board_wash_alphas_are_preserved() {
    let theme = Theme::from_text(BENCH);
    for depth in [Depth::Truecolor, Depth::Indexed, Depth::Sixteen] { assert_eq!(Theme::with_depth(BENCH, depth).background, GROUND); }
    assert_eq!(theme.cursor, ansi(&blend("#d28a41", "#1c120d", 0.18), true));
    assert_eq!(theme.selected, ansi(&blend("#c6b395", "#1c120d", 0.18), true));
    assert_eq!(theme.tab, ansi(&blend("#c6b395", "#1c120d", 0.08), true));
    assert_eq!(theme.border, ansi(&blend("#c6b395", "#1c120d", 0.12), false));
    assert_ne!(theme.selected, theme.marked);
    let p = Theme::from_text("color7='#112233'\ncolor0='#010203'\ncolor4='#445566'\naccent='#123456'\nred='#654321'\ncolor1='#abcdef'");
    assert_eq!(p.foreground, ansi("#112233", false));
    assert_eq!(p.background, GROUND);
    assert_eq!(p.selected, ansi(&blend("#112233", "#010203", 0.18), true));
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
#[test]
fn a_read_that_changes_before_its_second_look_is_not_published() {
    let root = crate::backend::testdir::TestDir::new("theme-settle");
    let path = root.file("colors.toml", "background='#ffffff'\nforeground='#00");
    root.assert_contains(&path);
    let full = "background='#ffffff'\nforeground='#000000'\n";
    assert_eq!(settled(&path, None, || root.replace_file(&path, full)), None, "a read cut after its first colour was published");
    assert_eq!(settled(&path, None, || {}).as_deref(), Some(full));
    assert_eq!(settled(&path, Some(full), || panic!("an unchanged read waits for a second look")), None);
}
// The prompt field is the foreground's 7% over the background; its stem wash eases from 24% only as far as foreground text needs.
#[test]
fn prompt_field_and_stem_wash_keep_text_contrast() {
    let theme = Theme::from_text(BENCH);
    let field = blend("#c6b395", "#1c120d", 0.07);
    assert_eq!(theme.field, ansi(&field, true));
    assert_eq!(theme.stem, ansi(&blend("#d28a41", &field, 0.24), true), "full strength where the text already reads");
    assert_eq!(theme.ext, ansi(&ensure(&contrast(&contrast("#68544b", "#1c120d"), "#150d0a"), &field, 4.5), false));
    let bright = Theme::from_text("background='#000000'\nforeground='#b0b0b0'\naccent='#ffffff'");
    let field = blend("#b0b0b0", "#000000", 0.07);
    assert!(ratio(rgb("#b0b0b0"), rgb(&blend("#ffffff", &field, 0.23))) < 4.5, "the fixture needs the easing");
    assert_eq!(bright.stem, ansi(&blend("#ffffff", &field, 0.22), true));
}
// The card frame eases to 3:1 under the menu wash, its rule is the track at 40%, and syntax inks lift to the ground's contrast.
#[test]
fn card_inks_take_the_board_track_frame_rule_and_syntax_values() {
    let theme = Theme::from_text(BENCH);
    let frame = ensure("#68544b", "#1c120d", 3.0);
    let outline = ensure(&frame, &blend("#c6b395", "#1c120d", 0.08), 3.0);
    // The board's #715e56 and #7a6860, within the one-step rounding between its script and this one.
    for (ink, board) in [(&frame, "#715e56"), (&outline, "#7a6860")] {
        assert!(rgb(ink).iter().zip(rgb(board)).all(|(a, b)| (a - b).abs() <= 1.0 / 255.0 + 1e-9), "{ink} against the board's {board}");
    }
    assert_eq!(theme.track, ansi(&frame, false));
    assert_eq!(theme.frame, ansi(&outline, false));
    assert_eq!(theme.rule, ansi(&blend(&frame, "#1c120d", 0.4), false));
    assert_eq!(theme.syntax[0], ansi(&contrast("#d65a4f", "#1c120d"), false));
    assert_eq!(theme.syntax[1], ansi(&contrast("#c6b395", "#1c120d"), false), "a missing palette ink falls back to the foreground");
    assert_eq!(theme.ground, "#1c120d");
}
#[test]
fn the_scrim_mixes_every_ink_halfway_to_the_ground() {
    let theme = Theme::from_text(BENCH);
    let scrim = theme.dim.as_deref().unwrap();
    assert_eq!(scrim.foreground, ansi(&blend("#c6b395", "#1c120d", 0.5), false));
    let outline = ensure(&ensure("#68544b", "#1c120d", 3.0), &blend("#c6b395", "#1c120d", 0.08), 3.0);
    assert_eq!(scrim.frame, ansi(&blend(&outline, "#1c120d", 0.5), false));
    assert_eq!(scrim.background, theme.background);
    assert!(scrim.dim.is_none());
    assert_eq!(Theme::with_depth(BENCH, Depth::Indexed).dim.unwrap().depth, Depth::Indexed);
}
