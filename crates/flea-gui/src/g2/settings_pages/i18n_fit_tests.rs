//! Catalog acceptance at the board's 1100x700 window and text size 14.
use super::{fit, COL_GAP, PAD};
use crate::g2::rail_places::rows::{i18n_fit_widths, HEADING_TRACKING};
use crate::list::Typeface;
use flea::jsondoc::{self, Json};
use flea::keys::SHEET_GROUPS;
use gpui::{font, px, Hsla, TextRun, TextSystem, WindowTextSystem};
use std::{borrow::Cow, collections::BTreeSet, fs, path::Path, sync::Arc};

fn text_system(native: bool) -> (WindowTextSystem, Typeface) {
    let text = Arc::new(TextSystem::new(
        gpui_platform::current_platform(true).text_system(),
    ));
    if !native {
        text.add_fonts(vec![Cow::Borrowed(
            include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
        )])
        .unwrap();
    }
    let face = Typeface {
        font: if native {
            Typeface::omarchy_default().font
        } else {
            font("JetBrains Mono")
        },
        ..Typeface::omarchy_default().at_base(14)
    };
    let id = text.resolve_font(&face.font);
    let family = text.font_family_name(id).unwrap();
    if native {
        assert!(
            matches!(
                family.as_ref(),
                "JetBrainsMono Nerd Font" | "JetBrainsMono NF"
            ),
            "OEM font unavailable: {family}"
        );
    } else {
        assert_eq!(family.as_ref(), "JetBrains Mono");
    }
    let advance = f32::from(text.advance(id, face.caption, '0').unwrap().width);
    assert!((advance - 7.2).abs() < 0.001, "fixture advance: {advance}");
    println!("SOL-I18N-FIT font={family}, caption=12, advance={advance}, native={native}");
    (WindowTextSystem::new(text), face)
}

fn width(text: &WindowTextSystem, face: &Typeface, value: &str, size: gpui::Pixels) -> f32 {
    let run = TextRun {
        len: value.len(),
        font: face.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = text.layout_line(value, size, &[run], None);
    if face.font.family.as_ref() != "JetBrains Mono" {
        for glyph in line.runs.iter().flat_map(|run| &run.glyphs) {
            let ch = value[glyph.index..].chars().next().unwrap();
            assert!(
                glyph.id.0 != 0 || matches!(ch, '\u{200c}' | '\u{200d}' | '\u{2066}'..='\u{2069}'),
                "OEM fallback lacks {ch:?} in {value}"
            );
        }
    }
    f32::from(line.width)
}

fn document(path: &Path) -> Json {
    jsondoc::parse(&fs::read_to_string(path).unwrap()).unwrap()
}

fn label<'a>(
    catalog: &'a Json,
    english: &'a Json,
    key: &str,
    code: &str,
    desktop: &'a Json,
) -> &'a str {
    let value = catalog
        .get(key)
        .and_then(Json::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| english.get(key).and_then(Json::as_str))
        .unwrap_or_else(|| panic!("non-text caption: {key}"));
    if value.starts_with("omarchy:") {
        if let Some(value) = desktop
            .get(code)
            .and_then(|aliases| aliases.get(value))
            .and_then(Json::as_str)
        {
            return value;
        }
        // The deterministic profile uses the same English alias fallback as I18n without host catalogs.
        match value {
            "omarchy:keybindings.Mute" => "Mute",
            "omarchy:keybindings.Network" => "Network",
            _ => panic!("unmeasured desktop alias: {key}: {value}"),
        }
    } else {
        assert!(english.get(key).is_some(), "unknown caption: {key}");
        value
    }
}

fn catalog_overflows(native: bool, heading_probe: Option<&str>) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let catalogs = root.join("i18n");
    let english = document(&catalogs.join("en/flea.json"));
    let boards = jsondoc::parse(include_str!("../../../tests/i18n-fit-boards.json")).unwrap();
    let roster = document(&root.join("src/i18n/fixtures/locales.json"));
    let (text, face) = text_system(native);
    let (_, _, card, _) = fit::rect(1100., 700., fit::MAX_SHEET_W);
    // Short action captions leave the card at 628, so every heading must fit its minimum column.
    let column = (super::SHEET_W - 2. * PAD - 2. - COL_GAP) / 2.;
    let caption = fit::caption_width(card);
    let (rail, heading) = i18n_fit_widths(&face);
    assert_eq!((caption, rail, heading), (166., 140., 164.));
    assert_eq!((f32::from(face.body), f32::from(face.caption)), (14., 12.));
    let actions: BTreeSet<_> = SHEET_GROUPS
        .iter()
        .flat_map(|group| group.actions.iter())
        .map(|action| format!("keys.action.{action}"))
        .collect();
    let mut paths: Vec<_> = fs::read_dir(&catalogs)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("flea.json").is_file())
        .collect();
    paths.sort();
    let codes: BTreeSet<_> = paths
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    let expected: BTreeSet<_> = roster
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, _)| key.clone())
        .collect();
    assert_eq!(codes, expected);
    assert_eq!(codes.len(), 50, "English and all 49 translations");
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in paths {
        let code = path.file_name().unwrap().to_str().unwrap();
        let catalog = document(&path.join("flea.json"));
        let desktop = if native {
            let file = Path::new("/usr/share/omarchy/default/i18n")
                .join(code)
                .join("keybindings.json");
            let values = document(&file);
            Json::Obj(vec![(
                code.to_owned(),
                Json::Obj(
                    values
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(key, value)| (format!("omarchy:keybindings.{key}"), value.clone()))
                        .collect(),
                ),
            )])
        } else {
            boards.get("aliases").unwrap().clone()
        };
        for (key, _) in english.as_object().unwrap() {
            let (size, slot, tracking, capitals) = if actions.contains(key) {
                (face.caption, caption, 0., false)
            } else if key.starts_with("keys.group.") {
                (face.caption, column, 1., true)
            } else if key.starts_with("places.row.")
                || flea::ui_logic_rail_places::tags::NAMES
                    .iter()
                    .any(|name| flea::ui_logic_rail_places::tags::label_key(name) == key)
                || key == "places.favorite.invalid_label"
            {
                (face.body, rail, 0., false)
            } else if key.starts_with("places.heading.")
                || matches!(
                    key.as_str(),
                    "settings.places.network" | "settings.places.devices"
                )
            {
                (face.caption, heading, HEADING_TRACKING, true)
            } else {
                continue;
            };
            let value = heading_probe
                .filter(|_| code == "en" && key == "keys.group.move")
                .unwrap_or_else(|| label(&catalog, &english, key, code, &desktop));
            if let Some(expected) = boards
                .get(code)
                .and_then(|labels| labels.get(key))
                .and_then(Json::as_str)
            {
                assert_eq!(
                    value, expected,
                    "{code}: {key}: approved board caption changed"
                );
            }
            let value = if key.starts_with("keys.group.") {
                flea::ui_logic_settings_pages::caps::heading(value, code)
            } else if capitals {
                value.to_uppercase()
            } else {
                value.to_owned()
            };
            let tracked = value
                .chars()
                .all(|c| c.is_ascii() || matches!(c as u32, 0xC0..=0x24F | 0x370..=0x52F));
            let needs = width(&text, &face, &value, size)
                + if tracked {
                    tracking * value.chars().count().saturating_sub(1) as f32
                } else {
                    0.
                };
            checked += 1;
            if needs > slot {
                failures.push(format!("{code}\t{key}\t{needs:.2}>{slot:.2}px\t{value}"));
            }
        }
    }
    println!(
        "SOL-I18N-FIT checked {checked} labels in 50 catalogs; caption={caption}px, rail={rail}px"
    );
    failures
}

fn catalogs_fit(native: bool) {
    let failures = catalog_overflows(native, None);
    assert!(
        failures.is_empty(),
        "{} overflowing labels:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn sol_i18n_fit_all_catalog_shortcuts_and_rail_at_text_14() {
    catalogs_fit(false);
}

#[test]
#[ignore = "requires native2's OEM fonts and pinned Omarchy desktop catalogs"]
fn sol_i18n_fit_native_oem_catalogs_at_text_14() {
    catalogs_fit(true);
}

#[test]
fn sol_i18n_fit_german_about_caption_keeps_the_exact_board_string() {
    use crate::g2::settings_pages::geometry::{pane_size, Scale, BOARD_CARD_H, CARD_FLOOR};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let german = document(&root.join("i18n/de/flea.json"));
    let board = jsondoc::parse(include_str!("../../../tests/i18n-fit-boards.json")).unwrap();
    let key = "settings.about.check_automatically_caption";
    let caption = german.get(key).and_then(Json::as_str).unwrap();
    assert_eq!(
        caption,
        board
            .get("de")
            .unwrap()
            .get(key)
            .and_then(Json::as_str)
            .unwrap()
    );
    let (text, face) = text_system(false);
    let pane = pane_size(CARD_FLOOR, BOARD_CARD_H, Scale::at(&face)).0;
    let slot = (pane * 0.5).round();
    assert!(width(&text, &face, caption, face.caption) <= slot);
    assert!(
        width(
            &text,
            &face,
            "beim Öffnen von Über, alle 6 Std.",
            face.caption
        ) > slot
    );
}

#[test]
fn sol_i18n_fit_german_and_russian_gui_tui_board_copy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let english = document(&root.join("i18n/en/flea.json"));
    let board = jsondoc::parse(include_str!("../../../tests/i18n-fit-boards.json")).unwrap();
    let desktop = board.get("aliases").unwrap();
    let mut drift = Vec::new();
    for code in ["de", "ru"] {
        let catalog = document(&root.join("i18n").join(code).join("flea.json"));
        for (key, expected) in board.get(code).unwrap().as_object().unwrap() {
            let actual = label(&catalog, &english, key, code, desktop);
            if actual != expected.as_str().unwrap() {
                drift.push(format!(
                    "{code}:{key}: {actual:?} != {:?}",
                    expected.as_str().unwrap()
                ));
            }
        }
    }
    assert!(
        drift.is_empty(),
        "{} board-copy differences:\n{}",
        drift.len(),
        drift.join("\n")
    );
}

#[test]
fn sol_i18n_fit_measured_column_boundaries() {
    let (text, face) = text_system(false);
    assert_eq!(
        fit::wanted_width(width(&text, &face, &"x".repeat(22), face.caption)),
        628.
    );
    assert_eq!(
        fit::wanted_width(width(&text, &face, &"x".repeat(23), face.caption)),
        642.
    );
    assert!(width(&text, &face, &"x".repeat(23), face.caption) <= fit::caption_width(642.));
    assert!(width(&text, &face, &"x".repeat(24), face.caption) > fit::caption_width(642.));
    assert!(width(&text, &face, &"x".repeat(16), face.body) <= 140.);
    assert!(width(&text, &face, &"x".repeat(17), face.body) > 140.);
    assert!(width(&text, &face, "Dossier personnel", px(14.)) > 140.);
    let failures = catalog_overflows(false, Some(&"x".repeat(36)));
    assert_eq!(
        failures,
        [format!(
            "en\tkeys.group.move\t294.20>290.00px\t{}",
            "X".repeat(36)
        )]
    );
}

#[test]
fn sol_i18n_fit_russian_board_width_through_the_rendered_sheet() {
    use super::SheetView;
    use crate::g2::settings_pages::sheet::{Sheet, Sources};
    use crate::g2::settings_pages::tests::english;
    use gpui::{TestAppContext, TestDispatcher};
    let platform = gpui_platform::current_platform(true).text_system();
    let mut app = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, platform);
    app.update(|cx| {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            )])
            .unwrap();
    });
    let (view, cx) = app.add_window_view(|_, cx| {
        let mut view = SheetView::new(Sheet::new("default", english(), Sources::default()), cx);
        view.typeface.font = font("JetBrains Mono");
        view
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |view, _| f32::from(
            view.probe.bounds("card").unwrap().size.width
        )),
        628.
    );
    view.update(cx, |view, cx| {
        for row in &mut view.sheet.rows {
            if row.action == "openTab" {
                row.label = "Открыть в новой вкладке".into();
            }
            if row.action == "tabMoveRight" {
                row.label = "Сдвинуть вкладку вправо".into();
            }
        }
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        view.read_with(cx, |view, _| f32::from(
            view.probe.bounds("card").unwrap().size.width
        )),
        642.
    );
}
