//! Localized labels keep their drawn line boxes when the Settings card narrows.
use super::geometry_tests::settings_in;
use flea::ui_logic_settings_pages::Text;

/// LANGUAGES-SPEC: a Settings label that would collide with its control wraps to a second line and the row grows to 51, never elides.
#[gpui::test]
fn i50_fit_settings_wrap_51(cx: &mut gpui::TestAppContext) {
    use flea::ui_logic_settings_pages::rows::Kind;
    const TODAY: &str = "Heutiges Datum hervorheben";
    const LONG_TODAY: &str = "Heutiges Datum hervorheben und den Dateinamen zeigen";
    let control = |kind: Kind| {
        matches!(
            kind,
            Kind::Fact | Kind::Lock | Kind::Check | Kind::Choice | Kind::Action
        )
    };
    // The board's rows, with a longer test translation to exercise wrapping above the 630px card floor.
    let board = [
        "Zuletzt genutzte Ansicht",
        "Ansicht je Ordner merken",
        "Adressleiste",
        "Spalten in der Spaltenansicht",
        "Git-Status anzeigen",
        LONG_TODAY,
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
    const NARROWEST: f32 = 640.;
    let mut width = BOARD_WIDTH + STEP;
    while width > NARROWEST {
        width -= STEP;
        let (view, cx) =
            cx.add_window_view(|_, cx| super::view::SettingsView::new(settings_in("de"), cx));
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(700.)));
        view.update(cx, |view, cx| {
            view.panel.show_section("view");
            let row = view
                .panel
                .rows
                .iter_mut()
                .find(|row| row.id == "highlightToday")
                .unwrap();
            assert_eq!(row.label.resolve(&view.panel.catalog), TODAY);
            row.label = Text::data(LONG_TODAY);
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
            .position(|label| label == LONG_TODAY)
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
            Some(label) if f32::from(label.size.height) < 2. * super::geometry::WRAP_LINE => {}
            drawn => panic!("at {width} the label (wraps {wraps}) is {drawn:?}"),
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
