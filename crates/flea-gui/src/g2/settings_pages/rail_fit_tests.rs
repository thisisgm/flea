//! A Settings rail label that is wider than its row is cut with an ellipsis inside the rail, never drawn past it.
use super::{geometry::PAD_X, view::SettingsView};
use flea::ui_logic_settings_pages::pages::SECTIONS;
use std::path::PathBuf;

fn settings_in(language: &str) -> super::panel::Panel {
    let store = super::seams::MemoryStore::new(flea::jsondoc::parse("{}").expect("fixture parses"));
    super::panel::Panel::new(
        Box::new(store),
        super::seams::Facts::default(),
        crate::row::fit_tests::catalog(language),
    )
}

fn languages() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../i18n");
    let mut found: Vec<String> = std::fs::read_dir(root)
        .expect("the catalogs")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .path()
                .join("flea.json")
                .is_file()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_rail_label_stays_inside_its_row_in_every_language() {
    // The platform's own shaper and fonts, so a Tamil label measures as it draws.
    let platform = gpui_platform::current_platform(true).text_system();
    let mut app =
        gpui::TestAppContext::build_with_text_system(gpui::TestDispatcher::new(0), None, platform);
    let cx = &mut app;
    let languages = languages();
    assert!(languages.iter().any(|language| language == "ta"));
    for language in &languages {
        let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(settings_in(language), cx));
        cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for section in SECTIONS {
            let id = section.id;
            let row = view
                .read_with(cx, |view, _| view.probe.bounds(&format!("rail:{id}")))
                .unwrap_or_else(|| panic!("{language}: the {id} rail row is drawn"));
            let selector: &'static str =
                Box::leak(format!("settings-rail-label-{id}").into_boxed_str());
            let label = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{language}: the {id} rail label is drawn"));
            let inside = f32::from(row.right()) - PAD_X + 0.5;
            assert!(
                f32::from(label.right()) <= inside,
                "{language}: the {id} label ends at {:?}, past the row's padding at {inside}",
                label.right()
            );
        }
    }
}
