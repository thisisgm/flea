//! The real card paints only its visible applications through the existing shared icon service.
use super::openwith_controls_tests::prepared;
use crate::g2::context_menus::app_icons;
use gpui::TestAppContext;
use std::collections::BTreeSet;

#[gpui::test]
fn openwith_card_rasters_and_paints_only_visible_application_marks(cx: &mut TestAppContext) {
    let paths: Vec<String> = (0..30)
        .map(|at| {
            format!(
                "{}/{}icon.svg",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images"),
                "./".repeat(at)
            )
        })
        .collect();
    let apps = paths
        .iter()
        .enumerate()
        .map(|(at, path)| {
            format!(
                r#"{{"id":"app{at}.desktop","label":"App {at:03}","icon":"{}","default":false}}"#,
                flea::json::escape(path)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    app_icons::PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    let (shell, cx, _sent) = prepared(cx, &apps);
    let requested: BTreeSet<_> = cx.update(|window, cx| {
        paths
            .iter()
            .filter(|path| app_icons::requested(path, window, cx))
            .cloned()
            .collect()
    });
    let expected: BTreeSet<_> = paths[..7].iter().cloned().collect();
    assert_eq!(
        requested, expected,
        "the card requests exactly its seven visible application icons"
    );
    let painted: BTreeSet<_> = app_icons::PAINTED
        .with(|painted| painted.borrow_mut().take().unwrap().into_iter().collect());
    assert_eq!(
        painted, expected,
        "the real row painter uses each requested image"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn openwith_fidelity_fallbacks_paint_with_the_muted_frame_role(cx: &mut TestAppContext) {
    let apps = r#"{"id":"a.desktop","label":"A","icon":"","default":true},{"id":"b.desktop","label":"B","icon":"unresolved-icon","default":false}"#;
    app_icons::FALLBACKS.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    let (shell, cx, _sent) = prepared(cx, apps);
    let expected = cx.update(|_, cx| {
        cx.try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default()
            .muted_frame
    });
    let painted = app_icons::FALLBACKS.with(|painted| painted.borrow_mut().take().unwrap());
    let paths: BTreeSet<_> = painted.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, BTreeSet::from(["", "unresolved-icon"]));
    assert!(
        painted.iter().all(|(_, ink)| *ink == expected),
        "empty and unresolved application icons use the shipped muted-frame window mark: {painted:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// v0.3.12 MenuRow.qml takes a file:// Icon= as it comes and sends a bare name to the themed lookup; both reach the same resolver as a path.
#[gpui::test]
fn openwith_card_paints_a_file_url_icon_through_the_themed_resolver(cx: &mut TestAppContext) {
    let url = format!(
        "file://{}",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images/icon.svg")
    );
    let apps = format!(
        r#"{{"id":"a.desktop","label":"A","icon":"{}","default":true}}"#,
        flea::json::escape(&url)
    );
    app_icons::PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    let (shell, cx, _sent) = prepared(cx, &apps);
    let painted: BTreeSet<_> = app_icons::PAINTED
        .with(|painted| painted.borrow_mut().take().unwrap().into_iter().collect());
    assert_eq!(
        painted,
        BTreeSet::from([url]),
        "the file URL icon is resolved and painted"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
