//! Original Glassy appearance keeps its translucent file area and independent readable inks.
use super::*;

const LATTE: &str = r##"mode = "light"
accent = "#1e66f5"
selection = "#ccd0da"
muted = "#acb0be"
background = "#eff1f5"
dark_background = "#e3e4e8"
darker_background = "#d7d8dc"
lighter_background = "#dce0e8"
foreground = "#4c4f69"
dark_foreground = "#9ca0b0"
light_foreground = "#5c5f77"
bright_foreground = "#4c4f69"
red = "#d20f39"
yellow = "#df8e1d"
orange = "#d84e2b"
green = "#40a02b"
cyan = "#179299"
blue = "#1e66f5"
magenta = "#ea76cb"
brown = "#6c2715"
bright_red = "#d20f39"
bright_yellow = "#df8e1d"
bright_green = "#40a02b"
bright_cyan = "#179299"
bright_blue = "#1e66f5"
bright_magenta = "#ea76cb"
"##;

fn latte(cx: &mut TestAppContext, name: &str) -> Json {
    let fixture = install(cx, name, &[(6, "")]);
    fixture.write(&fixture.path().join("input-0"), LATTE);
    let paths: Vec<_> = (0..7)
        .map(|at| fixture.path().join(format!("input-{at}")))
        .collect();
    let snapshot =
        crate::presentation::Snapshot::glass_fixture(&paths, fixture.path().join("font-cache"));
    cx.update(|cx| snapshot.install(cx));
    let handle = launch(cx);
    draw(handle, cx);
    let value = glass_value(handle, cx);
    stop(handle, cx);
    fixture.cleanup();
    value
}

#[gpui::test]
fn glass_visual_header_uses_the_listing_ground(cx: &mut TestAppContext) {
    let value = latte(cx, "glassy-visual-header");
    let grounds = value.get("grounds").unwrap();
    assert_eq!(grounds.get("header"), grounds.get("listing"), "{value:?}");
    assert_eq!(grounds.get("header"), grounds.get("chrome"));
    assert!(grounds.get("listing").and_then(Json::as_f64).unwrap() < 255.);
}

#[gpui::test]
fn glass_visual_listing_preserves_the_translucent_file_area(cx: &mut TestAppContext) {
    let value = latte(cx, "glassy-visual-listing");
    let requested = (value
        .get("requestedListAlpha")
        .and_then(Json::as_f64)
        .unwrap()
        * 255.)
        .round();
    let measured = (value.get("listAlpha").and_then(Json::as_f64).unwrap() * 255.).round();
    assert!(
        requested < 255.,
        "the file area must remain glass: {value:?}"
    );
    assert_eq!(measured, requested, "{value:?}");
    assert_eq!(
        value
            .get("grounds")
            .unwrap()
            .get("listing")
            .and_then(Json::as_f64),
        Some(measured)
    );
}

#[test]
fn glass_visual_secondary_inks_hold_over_selected_dark_and_bright_backdrops() {
    use flea::theme::{ratio, Color, Material};
    let core = flea::theme::Theme::resolve(LATTE, "", "", None, "monospace");
    let inputs = crate::window::glass::Inputs::resolve(LATTE, "", "", false, true);
    let inks = inputs.chrome_inks.unwrap();
    for at in [2, 3, 4, 5] {
        let ink = gpui::Rgba::from(inks[at]);
        assert_eq!(ink.a, 1.);
        let ink = Color::parse(&crate::ipc::rows::colour(inks[at])).unwrap();
        for backdrop in Material::from_colors(&core.colors, true).backdrops() {
            for plane in [core.colors.background, core.colors.surface] {
                let ground = plane
                    .alpha(inputs.wanted_alpha as f64 / 255.)
                    .over(backdrop)
                    .quantized();
                for wash in [
                    core.colors.foreground.alpha(0.),
                    core.colors.accent.alpha(crate::list::CURSOR_WASH as f64),
                    core.colors.foreground.alpha(crate::list::MARK_FILL as f64),
                ] {
                    let ground = wash.over(ground).quantized();
                    assert!(
                        ratio(ink, ground) >= 4.5,
                        "role {at} contrast {} on {:?}",
                        ratio(ink, ground),
                        ground
                    );
                }
            }
        }
    }
}

#[gpui::test]
fn glass_rail_highlight_fills_keep_the_original_palette(cx: &mut TestAppContext) {
    use crate::g2::rail_places::producer::Producer;
    use gpui::prelude::*;
    for (name, colors) in [
        (
            "rail-fill-dark",
            "background='#101315'\nforeground='#cacccc'",
        ),
        ("rail-fill-light", LATTE),
    ] {
        let fixture = install(cx, name, &[(6, "")]);
        fixture.write(&fixture.path().join("input-0"), colors);
        let paths: Vec<_> = (0..7)
            .map(|at| fixture.path().join(format!("input-{at}")))
            .collect();
        let snapshot =
            crate::presentation::Snapshot::glass_fixture(&paths, fixture.path().join("font-cache"));
        cx.update(|cx| snapshot.install(cx));
        let handle = cx.update(|cx| {
            cx.open_window(
                gpui::WindowOptions {
                    window_background: gpui::WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |window, cx| {
                    crate::window::glass::mount(
                        window,
                        gpui::WindowBackgroundAppearance::Transparent,
                        cx,
                    );
                    cx.new(|cx| {
                        let state = flea::jsondoc::parse(
                            r#"{"places":{"showHome":true,"showTrash":true}}"#,
                        )
                        .unwrap();
                        let mut producer = Producer::new(
                            Arc::new(crate::test_catalogs::english()),
                            &state,
                            "/bin/false".into(),
                            cx,
                        );
                        producer.set_polling(true, cx);
                        producer
                    })
                },
            )
            .unwrap()
        });
        let draw_rail = |cx: &mut TestAppContext| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        draw_rail(cx);
        for hovered in [false, true] {
            handle
                .update(cx, |producer, _, cx| {
                    let first = producer.laid.borrow().rows[0].index;
                    let mut model = producer.model.borrow_mut();
                    model.focused = false;
                    model.lit = (!hovered).then_some(first);
                    model.hovered = hovered.then_some(first);
                    drop(model);
                    cx.notify();
                })
                .unwrap();
            draw_rail(cx);
            handle.update(cx, |producer, window, cx| {
                let theme = cx.global::<crate::palette::Theme>();
                let laid = producer.laid.borrow();
                let row = &laid.rows[0];
                assert_ne!(laid.palette.unwrap().foreground, theme.foreground, "glyph ink must remain strengthened");
                let expected = theme.foreground.opacity(if hovered { crate::list::HOVER } else { crate::list::GREY_CURSOR });
                let scale = window.scale_factor();
                assert!(window.painted_quads().iter().any(|quad| {
                    (quad.bounds.origin.y.as_f32() - row.rect.y * scale).abs() < 0.01
                        && (quad.bounds.size.width.as_f32() - row.rect.width * scale).abs() < 0.01
                        && (quad.bounds.size.height.as_f32() - row.rect.height * scale).abs() < 0.01
                        && quad.background.as_solid() == Some(expected)
                }), "{name}: hovered={hovered}, actual rail fill must use original foreground {expected:?}");
            }).unwrap();
        }
        handle
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        fixture.cleanup();
    }
}
