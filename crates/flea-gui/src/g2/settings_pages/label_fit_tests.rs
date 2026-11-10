use super::{
    geometry as g,
    panel::Panel,
    seams::{Facts, MemoryStore},
    view::SettingsView,
};
use flea::ui_logic_settings_pages::{pages::SECTIONS, rows::Kind};

fn settings_labels_fit(native: bool) {
    let platform = gpui_platform::current_platform(true).text_system();
    let mut app =
        gpui::TestAppContext::build_with_text_system(gpui::TestDispatcher::new(0), None, platform);
    if !native {
        app.update(|cx| {
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(
                    include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
                )])
                .unwrap()
        });
    }
    let roster = flea::jsondoc::parse(include_str!(
        "../../../../../src/i18n/fixtures/locales.json"
    ))
    .unwrap();
    let mut checked = 0;
    let mut failures = Vec::new();
    for (code, _) in roster.as_object().unwrap() {
        let catalog = crate::row::fit_tests::catalog(code);
        let panel = Panel::new(
            Box::new(MemoryStore::new(flea::jsondoc::Json::Null)),
            Facts::default(),
            catalog,
        );
        let (view, cx) = app.add_window_view(|_, cx| {
            let mut view = SettingsView::new(panel, cx);
            view.typeface = view.typeface.at_base(14);
            if !native {
                view.typeface.font = gpui::font("JetBrains Mono");
            }
            view
        });
        cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
        for section in SECTIONS {
            view.update(cx, |view, _| view.panel.show_section(section.id));
            cx.update(|window, cx| {
                let view = view.read(cx);
                let pane = g::pane_size(g::CARD_W, 684., view.panel.scale).0;
                for row in &view.panel.rows {
                    if row.elide_right
                        || !matches!(
                            row.kind,
                            Kind::Fact | Kind::Lock | Kind::Check | Kind::Choice | Kind::Action
                        )
                    {
                        continue;
                    }
                    let label = row.label.resolve(&view.panel.catalog);
                    let segment = row.kind == Kind::Choice && view.fits(row, pane, window);
                    let room = view.room(row, segment, pane, window);
                    let lines = view.label_lines(row, None, pane, window);
                    checked += 1;
                    // label_lines answers 1 for a label it elides (a room under its longest word, or a wrap past LABEL_LINES).
                    if lines == 1
                        && view.shaped(label.clone(), view.typeface.body, window) > room.label
                    {
                        failures.push(format!(
                            "{code} {} {}: {label:?} elides in {}px",
                            section.id, row.id, room.label
                        ));
                    }
                }
            });
        }
    }
    assert!(checked > 2000, "only {checked} labels checked");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    println!("FX-I18N3 Settings: {checked} complete control labels, 50 catalogs, text size 14, native={native}");
}

#[test]
fn fxi18n3_all_settings_control_labels_fit_at_text_14() {
    settings_labels_fit(false);
}

#[test]
#[ignore = "requires the native desktop's OEM fonts"]
fn fxi18n3_native_settings_control_labels_fit_at_text_14() {
    settings_labels_fit(true);
}
