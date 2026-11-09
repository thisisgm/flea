//! S1A-6: in a window too small for a row's controls, every Settings label elides on one line and never vanishes.
use super::geometry as g;
use super::tests::panel;
use super::view::SettingsView;
use flea::ui_logic_settings_pages::rows::Kind;
use gpui::{px, size, TestAppContext};

#[gpui::test]
fn s1a6_labels_elide_and_never_vanish_in_a_tiny_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    for (width, height) in [(320., 200.), (360., 240.), (400., 260.)] {
        cx.simulate_resize(size(px(width), px(height)));
        view.update(cx, |view, cx| {
            view.panel.show_section("view");
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (rows, heights, floor, line) = view.read_with(cx, |view, _| {
            (
                view.panel.rows.clone(),
                view.panel.heights(),
                f32::from(view.typeface.body) * super::geometry::LABEL_FLOOR,
                view.panel.scale.row,
            )
        });
        let mut drawn = 0;
        for (at, row) in rows.iter().enumerate() {
            if !matches!(
                row.kind,
                Kind::Fact | Kind::Lock | Kind::Check | Kind::Choice | Kind::Action
            ) {
                continue;
            }
            assert!(
                heights[at] < 3. * line,
                "at {width}x{height} {} stacks {}px tall",
                row.id,
                heights[at]
            );
            let selector: &'static str =
                Box::leak(format!("settings-label-{}", row.id).into_boxed_str());
            let Some(label) = cx.debug_bounds(selector) else {
                continue;
            };
            drawn += 1;
            assert!(
                f32::from(label.size.width) >= floor - 0.5,
                "at {width}x{height} the {} label is {}px wide",
                row.id,
                f32::from(label.size.width)
            );
        }
        assert!(drawn > 0, "no View label was drawn at {width}x{height}");
    }
    // At 320x200 wrapAtEnds has room for its longest word, so only the LABEL_LINES cap keeps it on one line.
    cx.simulate_resize(size(px(320.), px(200.)));
    cx.update(|window, cx| {
        let view = view.read(cx);
        let row = view
            .panel
            .rows
            .iter()
            .find(|row| row.id == "wrapAtEnds")
            .expect("View lists wrapAtEnds");
        let s = g::Scale::at(&view.typeface);
        let width = g::pane_size(g::card_size(320., 200., 0., s).0, 0., s).0;
        let label = row.label.resolve(&view.panel.catalog);
        let word = view.longest_word(&label, window);
        let room = view.room(row, false, width, window).label;
        assert!(
            room >= word,
            "wrapAtEnds has room for its longest word at 320x200"
        );
        assert!(
            view.shaped(label, view.typeface.body, window) > room,
            "wrapAtEnds does not fit on one line at 320x200"
        );
        assert_eq!(
            view.label_lines(row, None, width, window),
            1,
            "a label past two lines keeps one elided line"
        );
    });
}
