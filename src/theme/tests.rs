use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

fn c(hex: &str) -> Color {
    Color::parse(hex).unwrap()
}

fn palette(name: &str) -> Theme {
    golden_tests::resolve(&golden_tests::cases()[&(name.to_owned(), 14)])
}

#[test]
fn theme_contrast_easing_repairs_rounding_and_preserves_passing_inks() {
    assert_eq!(ratio(Color::WHITE, Color::BLACK), 21.0);
    for (ink, bg) in [
        ("#555555", "#202020"),
        ("#aaaaaa", "#eeeeee"),
        ("#414868", "#1a1b26"),
        ("#f7768e", "#13141c"),
        ("#000000", "#000000"),
    ] {
        for need in [3.0, 4.5, 7.0] {
            let lifted = ensure_ratio(c(ink), c(bg), need);
            assert!(ratio(lifted, c(bg)) >= need, "{ink}/{bg}/{need}");
            if ratio(c(ink), c(bg)) >= need {
                assert_eq!(lifted, c(ink));
            }
        }
    }
}

#[test]
fn theme_starship_tags_keep_swatch_outside_old_green_hue_window() {
    let t = palette("starship");
    let green = t.palette["green"];
    // Starship's green is orange (hue about 37 degrees), outside the old green window of 80..170.
    let hue = 60.0 * ((green.g - green.b) / (green.r - green.b));
    assert!((30.0..45.0).contains(&hue));
    assert!(!t.tags.fallback);
    assert_eq!(t.tags.swatch_size, 14);
    for i in 0..7 {
        assert!(ratio(t.tags.colors[i].unwrap(), t.colors.background) >= 3.0);
        assert!(ratio(t.tags.menu_colors[i].unwrap(), t.colors.surface) >= 3.0);
    }
}

#[test]
fn theme_tag_missing_duplicate_or_grey_uses_digits_but_gray_is_allowed() {
    let t = palette("starship");
    for mutation in [0, 1, 2] {
        let mut p = t.palette.clone();
        match mutation {
            0 => {
                p.remove("red");
            }
            1 => {
                p.insert("red".into(), p["green"]);
            }
            _ => {
                p.insert("red".into(), c("#808080"));
            }
        }
        assert!(TagTheme::resolve(&p, t.colors.background, t.colors.surface).fallback);
    }
    let mut p = t.palette;
    p.insert("dark_foreground".into(), c("#777777"));
    assert!(!TagTheme::resolve(&p, t.colors.background, t.colors.surface).fallback);
}

#[test]
fn theme_parser_matches_commons_subset_and_user_overrides() {
    let colors = "foreground = '#ffffff' # comment\nbackground = #000000\ncolor4 = '#ff0000'\naccent = '#00ff00'\nred = '#ff0000'\ncolor1 = '#ff8000'\ninvalid = '#00000g'\n";
    let shell = "[font]\nbase-size = 12\nbody = 17\n[spacing]\nscale = 2\nscale-with-font = false\n[controls]\nselected-color = accent\nselected-fill-alpha = 0.25\n";
    let user = "[font] # note\nbase-size = '14px'\n[controls]\nselected-fill-alpha = 0.3\n";
    let t = Theme::resolve(colors, shell, user, None, "Example Font");
    assert_eq!(t.font.base_size, 14);
    assert_eq!(t.font.sizes["body"], 17);
    assert_eq!(t.font.resolved_family, "Example Font");
    assert_eq!(t.spacing.pixels["rowGap"], 16);
    assert_eq!(t.colors.accent, c("#00ff00"));
    assert_eq!(t.fills["style.selectedFill"], c("#00ff00").alpha(0.3));
    assert!(t.colors.error.g > 0.0, "last urgent assignment must win");
    let pinned = Theme::resolve(colors, shell, user, Some(12), "Example Font");
    assert_eq!(pinned.font.sizes["body"], 12);
    assert_eq!(pinned.font.sizes["bodySmall"], 11);
    assert_eq!(pinned.font.sizes["caption"], 10);
    let fourteen = Theme::resolve(colors, "[font]\nbase-size = 14", "", None, "monospace");
    assert_eq!(fourteen.font.sizes["bodySmall"], 13);
    assert_eq!(fourteen.font.sizes["caption"], 12);
    assert!(parse_shell("[font]\na = 1e2\nb = [1,2]\nc = \"#fff\" # yes").contains_key("font.c"));
    assert_eq!(parse_shell("[font]\na = 1e2\nb = [1,2]").len(), 0);
    assert!(!Theme::resolve("", "", "", None, "monospace").ready);
    let legacy = "[style]\nselected-color = urgent\n[controls]\nselected-color = accent";
    let ordered = Theme::resolve(
        colors,
        legacy,
        "[style]\nselected-color = foreground",
        None,
        "monospace",
    );
    assert_eq!(
        ordered.fills["style.selectedFill"].hex(),
        c("#00ff00").hex()
    );
}

#[test]
fn theme_all_marked_plates_keep_text_contrast() {
    for rows in golden_tests::cases().values() {
        let t = golden_tests::resolve(rows);
        let fill = t.fills["markedPlate"].over(t.colors.background).quantized();
        assert!(
            ratio(t.colors.foreground, fill) >= 4.5,
            "{}",
            t.colors.background.hex()
        );
        assert!((0.12..=0.35).contains(&t.numbers["markedPlateAlpha"]));
    }
}

#[test]
fn theme_material_and_live_decoration_options() {
    let mut t = palette("tokyo-night");
    let m = Material::from_colors(&t.colors, true);
    let alpha = m.floor(4.5);
    assert!(alpha >= 192);
    m.apply(&mut t.colors, alpha);
    for ink in [t.colors.foreground, t.colors.muted, t.colors.metadata] {
        assert!(m.worst(ink, alpha) >= 4.5);
    }
    t.decoration.apply_rounding(8.0);
    assert_eq!(t.decoration.corner_radius, 8);
    t.decoration.apply_rounding(0.0);
    assert_eq!(t.decoration.corner_radius, 0);
    t.decoration.apply_rounding(f64::NAN);
    assert_eq!(t.decoration.corner_radius, 0);
    t.decoration.apply_gaps(11.0);
    assert_eq!(t.decoration.gaps_out, 6);
}

// Every writing test gets a fresh marked mktemp root inside this unit's scratch directory.
fn scratch() -> PathBuf {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g05");
    assert!(parent.is_absolute());
    fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("case.XXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(parent));
    fs::write(root.join(".g05-test-root"), "g05 theme tests\n").unwrap();
    root
}

fn write(root: &Path, path: &Path, text: &str) {
    assert!(root.is_absolute() && root.join(".g05-test-root").is_file());
    assert!(path.is_absolute() && path.starts_with(root));
    assert!(!path
        .components()
        .any(|p| p == std::path::Component::ParentDir));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn theme_watcher_follows_theme_directory_replacement_font_and_new_override() {
    let root = scratch();
    let paths = ThemePaths::for_home(&root, None);
    write(
        &root,
        &paths.current.join("theme/colors.toml"),
        "background = '#000000'",
    );
    let mut watcher = ThemeWatcher::new(paths.clone());
    assert!(!watcher.poll());
    let old = paths.current.join("theme");
    let moved = paths.current.join("previous");
    assert!(
        old.is_absolute()
            && moved.is_absolute()
            && old.starts_with(&root)
            && moved.starts_with(&root)
    );
    assert!(root.join(".g05-test-root").is_file() && !moved.exists());
    fs::rename(&old, &moved).unwrap();
    write(
        &root,
        &paths.current.join("theme/colors.toml"),
        "background = '#ffffff'",
    );
    assert!(watcher.poll());
    assert!(!watcher.poll());
    write(&root, &paths.fontconfig, "font changed");
    assert!(watcher.poll());
    write(&root, &paths.user_shell, "[font]\nbase-size = 14");
    assert!(watcher.poll());
    let t = paths.load(None, "Updated Font").unwrap();
    assert_eq!(t.colors.background, Color::WHITE);
    assert_eq!(t.font.base_size, 14);
    assert_eq!(t.font.resolved_family, "Updated Font");
}

#[test]
fn theme_watcher_follows_symlinks_and_missing_targets() {
    let root = scratch();
    let paths = ThemePaths::for_home(&root, Some(&root.join("state")));
    let target = root.join("palette");
    write(&root, &target.join("colors.toml"), "background = '#202020'");
    fs::create_dir_all(&paths.current).unwrap();
    let mut watcher = ThemeWatcher::new(paths.clone());
    symlink(&target, paths.current.join("theme")).unwrap();
    assert!(watcher.poll());
    assert_eq!(
        paths.load(None, "monospace").unwrap().colors.background,
        c("#202020")
    );
    write(&root, &paths.current.join("theme.name"), "palette\n");
    assert!(watcher.poll());
    write(&root, &paths.toggles.join("window-glassy.lua"), "mode");
    assert!(watcher.poll());
    assert!(watcher
        .stamps()
        .iter()
        .any(|s| s.target.as_deref() == Some(target.as_path())));
    assert!(ThemePaths::for_home(&root.join("absent"), None)
        .load(None, "monospace")
        .is_err());
}
