//! An Arabic catalog mirrors the production window: the Listing reads Left and Right the other way and a closed folder's chevron points left.
use super::test_shell::{asked, land, open_root, open_root_at, row};
use crate::{
    state::Shell,
    window::{locale::Snapshot, u07_tests::read},
};
use flea::i18n::{I18n, Paths};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::sync::Arc;

#[path = "rtl_editor_tests.rs"]
mod editors;

// The keycaps fixture's Arabic, which its locale registry gives the rtl direction.
fn arabic(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/keys/fixtures/keycaps");
    let catalog = I18n::load(Paths {
        omarchy: root.clone(),
        language_file: root.join("language-ar"),
        catalogs: vec![root.join("catalogs")],
    });
    assert!(catalog.locale().right_to_left);
    shell.update(cx, |shell, cx| {
        let locale = Snapshot {
            revision: 2,
            catalog: Arc::new(catalog),
        };
        shell.apply_locale(locale, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.layout_mirrored()));
}

#[gpui::test]
fn i50_rtl_an_arabic_listing_opens_on_left_and_climbs_on_right(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_at(cx, "/files/beta");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 1);
    arabic(&shell, cx);
    cx.simulate_keystrokes("left");
    assert_eq!(
        asked(&shell, cx),
        "/files/beta/inner",
        "Left reads as Right and opens"
    );
    land(&shell, cx, "/files/beta/inner", vec![row("leaf", false)], 2);
    cx.simulate_keystrokes("right");
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "Right reads as Left and climbs"
    );
}

#[gpui::test]
fn i50_rtl_an_arabic_outline_draws_a_closed_folder_chevron_pointing_left(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/empty", true);
    child.depth = 1;
    land(&shell, cx, "/files", vec![folder, child], 1);
    arabic(&shell, cx);
    let values: Vec<f32> = read(&shell, cx, "rowRect", &["1"])
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let width = cx.update(|window, _| window.viewport_size().width);
    // The logical point outline_tests hovers, at its reflected place.
    let at = gpui::point(
        width - gpui::px(values[0] + 14. + 32. + 11.),
        gpui::px(values[1] + values[3] / 2.),
    );
    cx.simulate_mouse_move(at, None, gpui::Modifiers::default());
    cx.run_until_parked();
    assert_eq!(read(&shell, cx, "rowGlyph", &["1"]), "chevron-left");
}

#[path = "rtl_route_tests.rs"]
mod routes;

#[path = "rtl_settings_key_tests.rs"]
mod settings_keys;

#[path = "rtl_crumb_tests.rs"]
mod crumbs;
