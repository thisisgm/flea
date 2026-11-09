//! tests/settings-columns.qml and settings-tail.qml: band heights, and the pane's reveal at every host height.
use super::geometry::{
    estimated_lines, fade_shown, hint_line_count, hint_units, row_height, Scale, FADE, FOOTER_LINE,
    HINT_LINE,
};
use super::panel::Panel;
use super::tests::panel;
use flea::ui_logic_settings_pages::{
    pages::SECTIONS,
    rows::{focusable, Kind, Row},
    Text,
};

#[test]
fn adv_text_size_key_preview_at_11_keeps_both_rounded_padding_edges() {
    let row = Row {
        kind: Kind::KeyPreview,
        items: vec![Default::default(); 2],
        ..Row::default()
    };
    let scale = Scale::at(&crate::list::Typeface::omarchy_default().at_base(11));
    assert_eq!(
        row_height(&row, false, 1, scale),
        31.,
        "19 px chip plus the two independently rounded 6 px padding edges"
    );
}

#[test]
fn a_hint_band_is_4_over_18_a_line_and_2_under_and_the_footer_is_49_at_one_line() {
    let hint = Row::hint(Text::data("a"));
    assert_eq!(
        (
            row_height(&hint, false, 1, Scale::default()),
            row_height(&hint, false, 3, Scale::default())
        ),
        (24., 4. + 3. * HINT_LINE + 2.)
    );
    let footer = Row {
        footer: true,
        ..Row::hint(Text::data("Saved"))
    };
    assert_eq!(row_height(&footer, false, 1, Scale::default()), 49.);
    assert_eq!(
        row_height(&footer, false, 2, Scale::default()),
        49. + FOOTER_LINE
    );
}

// Where the row's first ink starts under its top, as the reveal rule reads it.
fn ink(row: &Row, first: bool) -> f32 {
    match row.kind {
        Kind::Group if first => 16.,
        Kind::Group => 27.,
        _ => 8.,
    }
}

fn walk(panel: &mut Panel, section: &'static str, height: f32) {
    panel.show_section(section);
    panel.laid_out((panel.pane.0, height), Vec::new());
    assert!(!panel.rows.is_empty(), "{section} builds its rows");
    for _ in 0..panel.rows.len() {
        let heights = panel.heights();
        let top: f32 = heights.iter().take(panel.cursor).sum();
        let bottom = top + heights[panel.cursor];
        let at = format!("{section} at {height}: row {}", panel.cursor);
        assert!(
            top >= panel.scroll - 0.01 && bottom <= panel.scroll + height + 0.01,
            "{at}: cursor inside"
        );
        let last = !panel.rows.iter().skip(panel.cursor + 1).any(focusable);
        if fade_shown(&heights, panel.scroll, height) && !last {
            assert!(
                bottom <= panel.scroll + height - FADE + 0.01,
                "{at}: fade over the cursor"
            );
            if let Some(next) = panel.rows.get(panel.cursor + 1) {
                assert!(
                    bottom + ink(next, false) + (FADE * 0.5).round()
                        <= panel.scroll + height + 0.01,
                    "{at}: the next row's ink shows under the fade"
                );
            }
        }
        panel.key("j", "j", false);
    }
}

#[test]
fn settings_hint_keeps_every_chord_whole_for_layout() {
    let raw =
        "Ctrl+Shift++ and Ctrl+Shift+- step through them; Ctrl+Shift+0 follows Omarchy again.";
    let units = hint_units(raw);
    // The laid-out units are the same words in the same order, spaces kept on each word.
    assert_eq!(units.concat(), raw);
    // Every chord is one whole unit, never a fragment the layout could wrap inside.
    for chord in ["Ctrl+Shift++ ", "Ctrl+Shift+- ", "Ctrl+Shift+0 "] {
        assert!(
            units.iter().any(|unit| unit == chord),
            "{chord} is not one unit in {units:?}"
        );
    }
    assert!(
        !units.iter().any(|unit| unit.starts_with('+')),
        "a chord fragment in {units:?}"
    );
    // Packed whole, a chord that does not fit moves to the next line instead of splitting.
    assert_eq!(hint_line_count(&[90., 20.], 100.), 2);
    assert_eq!(hint_line_count(&[40., 20.], 100.), 1);
    assert_eq!(hint_line_count(&[120.], 100.), 1);
}

#[test]
fn an_unspaced_wide_script_hint_still_wraps_between_characters() {
    assert_eq!(
        hint_units("按Ctrl+Shift+0逐级"),
        ["按", "Ctrl+Shift+0", "逐", "级"]
    );
    assert_eq!(hint_units("a 你好 b").concat(), "a 你好 b");
    assert_eq!(
        hint_units("か\u{3099}き"),
        ["か\u{3099}", "き"],
        "a voicing mark stays with its kana"
    );
    assert_eq!(
        hint_units("你好，世界。"),
        ["你", "好，", "世", "界。"],
        "closing punctuation never starts a line"
    );
    assert_eq!(
        hint_units("按「回车」"),
        ["按", "「回", "车」"],
        "opening punctuation never ends a line"
    );
    assert_eq!(
        hint_units("Ctrl+K, then"),
        ["Ctrl+K, ", "then"],
        "a chord keeps its own comma"
    );
    assert_eq!(
        hint_units("辻\u{E0100}道"),
        ["辻\u{E0100}", "道"],
        "a variation selector stays with its ideograph"
    );
    assert_eq!(
        hint_units("按(回车)"),
        ["按", "(回", "车)"],
        "an ASCII bracket never ends a line"
    );
    assert_eq!(
        hint_units("\u{1100}\u{1161}\u{11A8}가"),
        ["\u{1100}\u{1161}\u{11A8}", "가"],
        "a conjoined syllable stays whole"
    );
    let sentence = "按下组合键可以逐级放大或缩小文字并在需要时恢复跟随系统设置的大小";
    assert!(
        estimated_lines(sentence, 14., 100.) > 1,
        "thirty wide characters fit no 100px line"
    );
}

#[test]
fn every_section_reveals_its_cursor_at_every_host_height() {
    let mut panel = panel(
        r#"{"places":{"favourites":[{"label":"A","path":"/a"},{"label":"B","path":"/b"},{"label":"C","path":"/c"}]}}"#,
    );
    let mut cut = Vec::new();
    // Plugins' rows are the plugin contract's (st2), so the page stands empty until it joins.
    panel.show_section("plugins");
    assert!(panel.rows.is_empty(), "plugins draws no rows of its own");
    for section in SECTIONS
        .iter()
        .map(|section| section.id)
        .filter(|id| *id != "plugins")
        .chain(["columns"])
    {
        for height in (160..=640).step_by(7) {
            walk(&mut panel, section, height as f32);
        }
        let total: f32 = panel.heights().iter().sum();
        if total > 300. {
            cut.push(section);
        }
    }
    assert!(cut.contains(&"menus") && cut.len() > 1, "{cut:?}");
}

#[test]
fn ey422_the_settings_measures_follow_the_text_size() {
    let board = Scale::default();
    assert_eq!(
        [
            board.head,
            board.rail_row,
            board.row,
            board.of(super::geometry::CARD_W)
        ],
        [27., 29., 37., 630.],
        "at text size 14 the scale is the boards' own numbers"
    );
    let s = Scale::at(&crate::list::Typeface::omarchy_default().at_base(16));
    // v0.3.12 at 16: chromeHeight 31, railRowHeight 34, rowHeight 43.
    assert_eq!([s.head, s.rail_row, s.row], [31., 34., 43.]);
}

/// Settings in a shipped language's catalog, on an empty settings file.
fn settings_in(language: &str) -> super::panel::Panel {
    let store = super::seams::MemoryStore::new(flea::jsondoc::parse("{}").expect("fixture parses"));
    super::panel::Panel::new(
        Box::new(store),
        super::seams::Facts::default(),
        crate::row::fit_tests::catalog(language),
    )
}

/// LANGUAGES-SPEC: a Settings label that would collide with its control wraps to a second line and the row grows to 51, never elides.
#[gpui::test]
fn i50_fit_settings_wrap_51(cx: &mut gpui::TestAppContext) {
    use flea::ui_logic_settings_pages::rows::Kind;
    let control = |kind: Kind| {
        matches!(
            kind,
            Kind::Fact | Kind::Lock | Kind::Check | Kind::Choice | Kind::Action
        )
    };
    // The rows lang-de-settings shows; a longer one below its fold may wrap, as LANGUAGES-SPEC allows.
    let board = [
        "Zuletzt genutzte Ansicht",
        "Ansicht je Ordner merken",
        "Adressleiste",
        "Spalten in der Spaltenansicht",
        "Git-Status anzeigen",
        "Heutiges Datum hervorheben",
        "Ordner beim Ziehen \u{f6}ffnen",
        "Sortieren nach",
        "Ordner zuerst",
        "Nach Art gruppieren",
        "Versteckte Dateien anzeigen",
        "Versteckte Dateien zuletzt",
    ];
    // From the board's window down in steps to the first width where the today row collides, the board's narrow case.
    const BOARD_WIDTH: f32 = 1100.;
    const STEP: f32 = 20.;
    const NARROWEST: f32 = 440.;
    let mut width = BOARD_WIDTH + STEP;
    while width > NARROWEST {
        width -= STEP;
        let (view, cx) =
            cx.add_window_view(|_, cx| super::view::SettingsView::new(settings_in("de"), cx));
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(700.)));
        view.update(cx, |view, cx| {
            view.panel.show_section("view");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (rows, heights, labels) = view.read_with(cx, |view, _| {
            let catalog = &view.panel.catalog;
            (
                view.panel.rows.clone(),
                view.panel.heights(),
                view.panel
                    .rows
                    .iter()
                    .map(|row| row.label.resolve(catalog))
                    .collect::<Vec<_>>(),
            )
        });
        for label in board {
            assert!(labels.iter().any(|at| at == label), "View shows {label:?}");
        }
        let today = labels
            .iter()
            .position(|label| label == "Heutiges Datum hervorheben")
            .expect("View shows the German today row");
        assert_eq!(rows[today].kind, Kind::Check);
        let wraps = heights[today] > 37.;
        let first = width == BOARD_WIDTH;
        assert!(!first || !wraps, "the today row fits the board's window");
        if !first && !wraps {
            continue;
        }
        assert_eq!(heights[today], if wraps { 51. } else { 37. }, "at {width}");
        // The drawn label, not only the measure: two whole line boxes when it wraps, the one-line label otherwise.
        let selector: &'static str =
            Box::leak(format!("settings-label-{}", rows[today].id).into_boxed_str());
        match cx.debug_bounds(selector) {
            Some(label) if wraps => assert_eq!(
                f32::from(label.size.height),
                2. * super::geometry::WRAP_LINE,
                "the wrapped label draws two lines at {width}"
            ),
            None if !wraps => {}
            drawn => panic!("at {width} the wrapped label is {drawn:?}"),
        }
        let shown: Vec<usize> = (0..rows.len())
            .filter(|at| control(rows[*at].kind) && board.contains(&labels[*at].as_str()))
            .collect();
        if !wraps {
            for &at in &shown {
                assert_eq!(heights[at], 37., "{:?} collides at {width}", labels[at]);
            }
        }
        // Every control row is one line, or whole line boxes inside the padding, no more lines than unbroken pieces, none wider than its room.
        let words: Vec<(f32, f32, f32)> = cx.update(|window, cx| {
            let view = view.read(cx);
            let pane = view.panel.pane.0;
            rows.iter()
                .zip(&labels)
                .map(|(row, label)| {
                    let segment = row.kind == Kind::Choice && view.fits(row, pane, window);
                    let pieces = super::wrap::unbroken(label);
                    let word = pieces
                        .iter()
                        .map(|word| view.shaped((*word).to_owned(), view.typeface.body, window))
                        .fold(0., f32::max);
                    let room = view.room(row, segment, pane, window).label;
                    (word, room, pieces.len() as f32)
                })
                .collect()
        });
        for ((at, row), &(word, room, pieces)) in rows.iter().enumerate().zip(&words) {
            let lines =
                (heights[at] - 2. * super::geometry::ROW_PAD_Y) / super::geometry::WRAP_LINE;
            let whole = lines >= 2. && lines.fract() == 0. && lines <= pieces && word <= room;
            assert!(
                !control(row.kind) || heights[at] == 37. || whole,
                "{:?} ({:?}) at {width} is {} with a {word}px word in {room}px",
                labels[at],
                row.id,
                heights[at]
            );
        }
        if wraps {
            assert!(
                shown.iter().any(|at| heights[*at] == 37.),
                "a label that fits keeps the 37px row at {width}"
            );
            return;
        }
    }
    panic!("the today row never collides down to {NARROWEST}px");
}

/// lang-caps: German ß becomes SS, Turkish i becomes İ and Greek drops its accents, in Settings headings and inside a typed run.
#[test]
fn i50_fit_heading_tr_el_de() {
    use super::view_head::heading_text;
    for (language, key, heading) in [
        ("de", "settings.group.view", "ANSICHT"),
        ("tr", "settings.group.view", "GÖRÜNÜM"),
        ("tr", "settings.places.devices", "CİHAZLAR"),
        ("el", "settings.group.view", "ΠΡΟΒΟΛΗ"),
        ("el", "settings.places.devices", "ΣΥΣΚΕΥΕΣ"),
    ] {
        let catalog = crate::row::fit_tests::catalog(language);
        assert_eq!(
            heading_text(&catalog.tr(key), None, language).0,
            heading,
            "{language} {key}"
        );
    }
    // ß doubles inside a typed run, and the run counts the capitals drawn.
    assert_eq!(
        heading_text("Größe", Some((3, 1)), "de"),
        ("GRÖSSE".to_owned(), Some((3, 2)))
    );
    assert_eq!(
        heading_text("Bilgi", Some((3, 2)), "tr"),
        ("BİLGİ".to_owned(), Some((3, 2)))
    );
}

/// lang-ja-list and lang-ar-list: a caseless language draws its headings plain, with no capitals and no tracking, even where a string falls back to English.
#[gpui::test]
fn i50_fit_caseless_no_tracking(cx: &mut gpui::TestAppContext) {
    use super::view_head::heading_text;
    for language in ["ja", "zh-CN", "ko", "ar", "fa", "he", "ur", "th"] {
        let catalog = crate::row::fit_tests::catalog(language);
        let view_title = catalog.tr("settings.group.view");
        assert_eq!(
            heading_text(&view_title, None, language).0,
            view_title,
            "{language}"
        );
        assert_eq!(
            heading_text("Display", Some((0, 3)), language),
            ("Display".to_owned(), Some((0, 3))),
            "{language}"
        );
        let (view, cx) =
            cx.add_window_view(|_, cx| super::view::SettingsView::new(settings_in(language), cx));
        let spacing = view.read_with(cx, |view, _| view.heading_spacing());
        assert_eq!(spacing, 0., "{language} headings take no tracking");
    }
    let (view, cx) =
        cx.add_window_view(|_, cx| super::view::SettingsView::new(settings_in("de"), cx));
    assert!(view.read_with(cx, |view, _| view.heading_spacing()) > 0.);
    assert_eq!(heading_text("Ansicht", None, "de").0, "ANSICHT");
}
