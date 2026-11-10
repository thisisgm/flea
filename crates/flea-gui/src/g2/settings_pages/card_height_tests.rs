//! S2-12: the card is as tall as View drawn at the card's width, wrapped hints and labels included, from every section.
use super::tests::panel;
use super::view::SettingsView;
use gpui::{px, size, TestAppContext};

fn wrapping_panel() -> super::panel::Panel {
    use std::{
        fs,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "flea-card-height-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join(".flea-test-sandbox"), "Settings hint fixture").unwrap();
    fs::create_dir(root.join("en")).unwrap();
    let hint =
        "A long localized hint keeps whole words and grows the card to hold every drawn line. "
            .repeat(3);
    fs::write(
        root.join("en/flea.json"),
        format!(r#"{{"settings.hint.compact_default":"{hint}"}}"#),
    )
    .unwrap();
    let mut panel = panel("{}");
    panel.catalog = Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/i18n/fixtures/omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.clone()],
    }));
    assert!(
        root.is_absolute()
            && root.starts_with(std::env::temp_dir())
            && root.join(".flea-test-sandbox").is_file()
    );
    fs::remove_dir_all(&root).unwrap();
    panel
}

#[gpui::test]
fn s2_12_the_card_holds_view_as_drawn_where_its_hints_wrap(cx: &mut TestAppContext) {
    // The width floor keeps today's English hints on one line, so a long catalog hint exercises wrapping.
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(wrapping_panel(), cx));
    let mut wrapped = false;
    for width in (520..=1100).step_by(20) {
        // Tall enough that the window never caps the card.
        cx.simulate_resize(size(px(width as f32), px(1600.)));
        view.update(cx, |view, cx| {
            view.panel.show_section("view");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (drawn, card, lines) = view.read_with(cx, |view, _| {
            let panel = &view.panel;
            (
                panel.heights().iter().sum::<f32>(),
                panel.view_height,
                panel.lines.clone(),
            )
        });
        wrapped |= lines.iter().any(|&count| count > 1);
        assert!(
            (drawn - card).abs() < 0.5,
            "at {width}px View draws {drawn}px but the card was sized for {card}px"
        );
        view.update(cx, |view, cx| {
            view.panel.show_section("display");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let elsewhere = view.read_with(cx, |view, _| view.panel.view_height);
        assert!(
            (elsewhere - card).abs() < 0.5,
            "at {width}px the card is {elsewhere}px on Display and {card}px on View"
        );
    }
    assert!(
        wrapped,
        "no width wrapped a View line, so this test proves nothing"
    );
}
