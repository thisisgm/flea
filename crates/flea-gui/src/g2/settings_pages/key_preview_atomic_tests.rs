//! Every first painted preview keeps two complete cells per band and its trailing hint clear.
use super::{geometry as g, panel::Press, tests::panel, view::SettingsView};
use flea::ui_logic_settings_pages::rows::Kind;
use gpui::{px, size, Entity, TestAppContext, VisualTestContext};

fn selector(prefix: &str, at: usize) -> &'static str {
    Box::leak(format!("{prefix}-{at}").into_boxed_str())
}

fn first_frame(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    phase: &str,
    scale: f32,
    reference_gap: Option<f32>,
) -> f32 {
    // No parked executor, next-frame callback or second draw may correct this frame.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (items, hint, row, metrics) = view.read_with(cx, |view, _| {
        let preview = view
            .panel
            .rows
            .iter()
            .find(|row| row.kind == Kind::KeyPreview)
            .unwrap();
        (
            preview.items.clone(),
            view.probe.bounds("row:").unwrap(),
            view.probe.bounds("row:keyPreview").unwrap(),
            view.panel.scale,
        )
    });
    // Rows stack with no gap, so the row read as the hint must start where the preview ends.
    assert!(
        (f32::from(hint.top()) - f32::from(row.bottom())).abs() < 0.01,
        "{phase}: row: is not the row under the preview: {hint:?} after {row:?}"
    );
    let padding = metrics.of(g::PAD_X);
    let vertical_padding = metrics.of(7.);
    let cells: Vec<_> = (0..items.len())
        .map(|at| {
            cx.debug_bounds(selector("settings-key-preview-keyPreview", at))
                .unwrap()
        })
        .collect();
    let tolerance = 1. / scale;
    for (band, pair) in cells.chunks(2).enumerate() {
        if pair.len() == 2 {
            assert_eq!(
                pair[0].top(),
                pair[1].top(),
                "{phase}: FIRST band {band} must have two columns"
            );
            assert!(
                pair[0].right() <= pair[1].left(),
                "{phase}: neighbours overlap: {pair:?}"
            );
        }
        if band > 0 {
            assert!(
                pair[0].top() >= cells[(band - 1) * 2].bottom(),
                "{phase}: bands overlap"
            );
            if scale == 1. {
                assert!(
                    (f32::from(pair[0].top() - cells[(band - 1) * 2].bottom())
                        - metrics.of(g::GAP))
                    .abs()
                        < 0.01
                );
            }
        }
    }
    for (at, cell) in cells.iter().enumerate() {
        if scale == 1. {
            assert!((f32::from(cell.size.height) - metrics.of(g::CHIP_H)).abs() < 0.01);
        }
        assert!(
            f32::from(cell.left()) + tolerance >= f32::from(row.left()) + padding
                && f32::from(cell.right()) <= f32::from(row.right()) - padding + tolerance
                && f32::from(cell.top()) + tolerance >= f32::from(row.top()) + vertical_padding
                && f32::from(cell.bottom())
                    <= f32::from(row.bottom()) - vertical_padding + tolerance,
            "{phase}: cell {at} clipped by preview: {cell:?} in {row:?}"
        );
        let label = cx
            .debug_bounds(selector("settings-key-label-keyPreview", at))
            .unwrap();
        let ink_width = cx.update(|window, cx| {
            let view = view.read(cx);
            view.shaped(
                items[at].label.resolve(&view.panel.catalog),
                view.typeface.caption,
                window,
            )
        });
        assert!(
            label.left() >= cell.left()
                && label.top() >= cell.top()
                && label.bottom() <= cell.bottom()
                && f32::from(label.left()) + ink_width <= f32::from(cell.right()) + 0.01,
            "{phase}: label {:?} ({ink_width}px) does not fit cell {at}: {label:?} in {cell:?}",
            items[at].label
        );
        assert!(
            f32::from(label.left()) + tolerance >= f32::from(row.left()) + padding
                && f32::from(label.left()) + ink_width
                    <= f32::from(row.right()) - padding + tolerance
                && f32::from(label.top()) + tolerance >= f32::from(row.top()) + vertical_padding
                && f32::from(label.bottom())
                    <= f32::from(row.bottom()) - vertical_padding + tolerance,
            "{phase}: FIRST label {:?} is clipped by padded preview {row:?}: {label:?}",
            items[at].label
        );
    }
    let bottom = cells
        .iter()
        .map(|cell| f32::from(cell.bottom()))
        .reduce(f32::max)
        .unwrap();
    let gap = f32::from(hint.top()) - bottom;
    assert!(
        gap >= 0.,
        "{phase}: FIRST hint overlaps the last band by {}",
        -gap
    );
    if let Some(reference) = reference_gap {
        assert!(
            (gap - reference).abs() <= tolerance,
            "{phase}: FIRST gap {gap}, scale-1 gap {reference}, tolerance {tolerance}"
        );
    }
    if !phase.contains("text=") {
        println!(
            "FX482 FIRST {phase}: last bottom {bottom}, hint top {:?}, gap {gap}",
            hint.top()
        );
    }
    gap
}

#[gpui::test]
fn fx482_first_frame_has_two_unclipped_columns_and_complete_labels(cx: &mut TestAppContext) {
    let mut checked = 0;
    for preset in ["default", "vim", "mac", "windows"] {
        let document = format!(r#"{{"keys":"{preset}"}}"#);
        let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel(&document), cx));
        for body in [14, 20] {
            let mut reference = None;
            for scale in [1., 0.5, 0.75, 1.25, 1.5, 1.6, 2.] {
                cx.simulate_scale_factor_change(scale);
                for width in [650., 651., 1100.] {
                    cx.simulate_resize(size(px(width), px(1000.)));
                    view.update(cx, |view, cx| {
                        view.typeface = crate::list::Typeface::omarchy_default().at_base(body);
                        view.panel.set_scale(g::Scale::at(&view.typeface));
                        view.panel.show_section("keys");
                        cx.notify();
                    });
                    let phase = format!("{preset} text={body} width={width} scale={scale}");
                    let gap = first_frame(&view, cx, &phase, scale, reference);
                    if reference.is_none() {
                        reference = Some(gap);
                    }
                    checked += 1;
                }
            }
        }
    }
    println!(
        "FX482 FIRST matrix: {checked} cases, two columns, complete labels, no clipping or overlap"
    );
}

#[gpui::test]
fn fx482_first_frame_is_atomic_after_open_resize_preset_and_language_change(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel("{}"), cx));
    cx.simulate_resize(size(px(650.), px(1000.)));
    view.update(cx, |view, cx| {
        view.panel.show_section("keys");
        cx.notify();
    });
    let gap = first_frame(&view, cx, "scale-1 reference", 1., None);
    cx.simulate_scale_factor_change(1.6);
    first_frame(&view, cx, "open at 650", 1.6, Some(gap));
    for width in [1100., 650.] {
        cx.simulate_resize(size(px(width), px(1000.)));
        first_frame(&view, cx, &format!("resize to {width}"), 1.6, Some(gap));
    }
    for preset in 1..=3 {
        view.update(cx, |view, cx| {
            view.panel.press(1, Press::Pick(preset));
            cx.notify();
        });
        first_frame(&view, cx, "preset change", 1.6, Some(gap));
    }
    let catalog = super::geometry_tests::settings_in("de").catalog;
    view.update(cx, |view, cx| {
        view.panel.set_catalog(catalog);
        cx.notify();
    });
    first_frame(&view, cx, "language change", 1.6, Some(gap));
}
