//! COPYTO-EDITOR-CONTRACT-1022 proves full editable text after real paint; former static ellipsis premise and successful proof remain in the 3574 archive.
use super::{
    fix10_tests::{init, real_app, redraw},
    host::Overlay,
    palette::CardPalette,
};
use crate::{list::Typeface, palette::Theme};
use gpui::{px, size, AppContext, Background, EntityInputHandler, Modifiers};
use std::sync::Arc;

fn long_path() -> String {
    format!("/{}", "very-long-destination-segment/".repeat(40))
}

// Mirror pinned Window::snap_bounds: round each edge, with half ties toward zero.
fn device_quad(bounds: gpui::Bounds<gpui::Pixels>, scale: f32) -> gpui::Bounds<gpui::ScaledPixels> {
    let edge = |value: gpui::Pixels| {
        let value = f32::from(value) * scale;
        (value.abs() - 0.5).ceil().copysign(value)
    };
    let left = edge(bounds.left());
    let top = edge(bounds.top());
    let right = edge(bounds.right()).max(left);
    let bottom = edge(bounds.bottom()).max(top);
    gpui::Bounds::from_corners(
        gpui::point(gpui::ScaledPixels(left), gpui::ScaledPixels(top)),
        gpui::point(gpui::ScaledPixels(right), gpui::ScaledPixels(bottom)),
    )
}

// Window::cover_bounds covers the content mask, independently of quad snapping.
fn device_mask(bounds: gpui::Bounds<gpui::Pixels>, scale: f32) -> gpui::Bounds<gpui::ScaledPixels> {
    let left = (f32::from(bounds.left()) * scale).floor();
    let top = (f32::from(bounds.top()) * scale).floor();
    let right = (f32::from(bounds.right()) * scale).ceil().max(left);
    let bottom = (f32::from(bounds.bottom()) * scale).ceil().max(top);
    gpui::Bounds::from_corners(
        gpui::point(gpui::ScaledPixels(left), gpui::ScaledPixels(top)),
        gpui::point(gpui::ScaledPixels(right), gpui::ScaledPixels(bottom)),
    )
}

#[test]
fn u13_join_mounted_editor_paint_mask_selection_caret_hit_and_ime_matrix() {
    let path = long_path();
    for moving in [false, true] {
        for base in [12., 14., 16.] {
            for width in [1100., 360.] {
                let mut app = real_app();
                init(&mut app);
                app.update(|cx| {
                    let face = cx.global::<Typeface>().clone().at_base(base as u32);
                    cx.set_global(face);
                });
                let (view, cx) = app.add_window_view(|window, _| {
                    window.set_rem_size(px(base));
                    let palette =
                        CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
                    Overlay::new(
                        Arc::new(crate::test_catalogs::english()),
                        palette,
                        "default",
                    )
                });
                cx.simulate_resize(size(px(width), px(700.)));
                view.update(cx, |view, cx| view.copy_to(71, moving, &path, cx));
                cx.run_until_parked();
                redraw(cx);
                let field = cx.debug_bounds("op1-field").unwrap();
                let paint = view.read_with(cx, |view, cx| {
                    let drawn = view.editor.drawn.borrow();
                    let paint = drawn.paint.clone().expect("actual Destination::paint");
                    let face = cx.global::<Typeface>();
                    assert_eq!(paint.font, face.font);
                    assert_eq!(paint.pixels, px(base));
                    assert_eq!(
                        drawn.line.run.as_ref().unwrap().line().font_size,
                        px(base),
                        "actual shaped font size, not a trace of the requested size"
                    );
                    assert_eq!(drawn.font.as_ref(), Some(&face.font));
                    assert_eq!(view.editor.entry.selected, 0..path.len());
                    assert_eq!(drawn.shaped_text.as_ref(), path);
                    assert_eq!(
                        drawn.line.run.as_ref().unwrap().line().len,
                        drawn.shaped_text.len()
                    );
                    assert!(
                        drawn.line.run.as_ref().unwrap().resolves() > 0,
                        "the actual cached run reached glyph paint"
                    );
                    assert!(
                        drawn.line.run.as_ref().unwrap().line().width > paint.bounds.size.width
                    );
                    assert_eq!(paint.origin, paint.bounds.left() - paint.scroll);
                    assert!(
                        paint.mask.top() >= field.top() && paint.mask.bottom() <= field.bottom()
                    );
                    assert_eq!(
                        paint.selection.unwrap().size.width,
                        drawn.line.x_for(path.len())
                    );
                    paint
                });
                assert_eq!(
                    paint.text, path,
                    "the actual shaped run retains all input bytes"
                );
                assert!(paint.mask.left() > field.left() && paint.mask.right() < field.right());
                assert_eq!(paint.mask, paint.bounds);
                assert_eq!(paint.input_bounds, Some(paint.bounds));
                assert!(
                    paint.selection.unwrap().left() < paint.mask.left(),
                    "opening Entry end scrolls the full selection"
                );
                assert!(paint.caret.is_none());
                let palette = view.read_with(cx, |view, _| view.palette);
                cx.update(|window, _| {
                    let scale = window.scale_factor();
                    assert!(window.painted_quads().iter().any(|q| {
                        q.bounds == device_quad(paint.selection.unwrap(), scale)
                            && q.content_mask.bounds == device_mask(paint.mask, scale)
                            && q.background == Background::from(palette.accent.opacity(0.24))
                    }));
                });
                cx.simulate_keystrokes("home");
                redraw(cx);
                view.read_with(cx, |view, _| {
                    assert_eq!(view.editor.entry.cursor(), 0);
                    let drawn = view.editor.drawn.borrow();
                    let paint = drawn.paint.as_ref().unwrap();
                    assert_eq!(paint.scroll, px(0.));
                    assert_eq!(paint.caret.unwrap().left(), paint.bounds.left());
                });
                let caret_paint = view.read_with(cx, |view, _| {
                    view.editor.drawn.borrow().paint.clone().unwrap()
                });
                cx.update(|window, _| {
                    let scale = window.scale_factor();
                    assert!(window.painted_quads().iter().any(|q| q.bounds
                        == device_quad(caret_paint.caret.unwrap(), scale)
                        && q.content_mask.bounds == device_mask(caret_paint.mask, scale)
                        && q.background == Background::from(palette.foreground)));
                });
                for (key, cursor) in [
                    ("right", 1),
                    ("left", 0),
                    ("end", path.len()),
                    ("left", path.len() - 1),
                    ("right", path.len()),
                ] {
                    cx.simulate_keystrokes(key);
                    assert_eq!(
                        view.read_with(cx, |view, _| view.editor.entry.cursor()),
                        cursor,
                        "copy_key must move the cursor after {key}"
                    );
                }
                redraw(cx);
                view.read_with(cx, |view, _| {
                    assert_eq!(view.editor.entry.cursor(), path.len());
                    assert!(view.editor.drawn.borrow().line.scroll > px(0.));
                });
                let (bounds, expected) = view.read_with(cx, |view, _| {
                    let drawn = view.editor.drawn.borrow();
                    let bounds = drawn.bounds.unwrap();
                    (
                        bounds,
                        drawn
                            .line
                            .run
                            .as_ref()
                            .unwrap()
                            .line()
                            .closest_index_for_x(drawn.line.scroll + px(2.)),
                    )
                });
                cx.simulate_click(
                    gpui::point(bounds.left() + px(2.), bounds.center().y),
                    Modifiers::none(),
                );
                assert_eq!(
                    view.read_with(cx, |view, _| view.editor.entry.cursor()),
                    expected
                );
                cx.simulate_mouse_down(bounds.origin, gpui::MouseButton::Left, Modifiers::none());
                cx.simulate_mouse_move(
                    bounds.bottom_right(),
                    Some(gpui::MouseButton::Left),
                    Modifiers::none(),
                );
                cx.simulate_mouse_up(
                    bounds.bottom_right(),
                    gpui::MouseButton::Left,
                    Modifiers::none(),
                );
                assert!(view.read_with(cx, |view, _| !view.editor.entry.selected.is_empty()));
                cx.simulate_keystrokes("ctrl-a");
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        let preedit = format!("/{}\u{1d11e}", "preedit/".repeat(120));
                        let units = preedit.encode_utf16().count();
                        view.replace_and_mark_text_in_range(
                            None,
                            &preedit,
                            Some(units..units),
                            window,
                            cx,
                        );
                        assert_eq!(view.marked_text_range(window, cx), Some(0..units));
                    })
                });
                redraw(cx);
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        let units = view.editor.entry.text.encode_utf16().count();
                        let drawn = view.editor.drawn.borrow();
                        let bounds = drawn.bounds.unwrap();
                        drop(drawn);
                        let candidate = view
                            .bounds_for_range(units..units, bounds, window, cx)
                            .unwrap();
                        assert!(
                            candidate.left() >= bounds.left()
                                && candidate.right() <= bounds.right()
                        );
                        let mut adjusted = None;
                        assert_eq!(
                            view.text_for_range(units - 2..units, &mut adjusted, window, cx)
                                .as_deref(),
                            Some("\u{1d11e}")
                        );
                    })
                });
                cx.simulate_keystrokes("enter");
                assert!(
                    view.read_with(cx, |view, _| view.editor.entry.marked.is_some()
                        && view.model.outbox.is_empty())
                );
                cx.simulate_keystrokes("tab");
                redraw(cx);
                let (paint, palette) = view.read_with(cx, |view, _| {
                    (
                        view.editor.drawn.borrow().paint.clone().unwrap(),
                        view.palette,
                    )
                });
                assert!(
                    paint.selection.is_none()
                        && paint.caret.is_none()
                        && paint.input_bounds.is_none()
                );
                assert_eq!(paint.mask, paint.bounds);
                cx.update(|window, _| {
                    let scale = window.scale_factor();
                    let quads = window.painted_quads();
                    assert!(quads.iter().any(|q| q.bounds == device_quad(field, scale)
                        && q.border_color == palette.muted_frame));
                    assert!(!quads
                        .iter()
                        .any(|q| q.background == Background::from(palette.accent.opacity(0.24))));
                });
            }
        }
    }
}

#[test]
fn u13_join_actual_editor_glyph_pixels_are_clipped_to_the_mounted_mask() {
    // Uses the stock renderer, without the line/raster recorder. Omitted paint must fail.
    for moving in [false, true] {
        for base in [12., 14., 16.] {
            for width in [1100., 360.] {
                let mut app = super::fix11_paint_tests::app();
                let path = long_path();
                let handle = app
                    .open_window(size(px(width), px(700.)), |window, cx| {
                        window.set_scale_factor(1.);
                        window.set_rem_size(px(base));
                        let palette = CardPalette::from_theme(
                            &window.text_style(),
                            *cx.global::<Theme>(),
                            8.,
                        );
                        cx.new(|cx| {
                            let mut view = Overlay::new(
                                Arc::new(crate::test_catalogs::english()),
                                palette,
                                "default",
                            );
                            view.copy_to(71, moving, &path, cx);
                            view
                        })
                    })
                    .unwrap();
                app.run_until_parked();
                app.update_window(handle.into(), |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .unwrap();
                let (mask, field) = handle
                    .update(&mut app, |view, window, _| {
                        let mask = view.editor.drawn.borrow().paint.as_ref().unwrap().mask;
                        let covered = device_mask(mask, window.scale_factor());
                        let field = window
                            .painted_quads()
                            .iter()
                            .find(|quad| {
                                quad.border_color == view.palette.accent_frame
                                    && covered.is_contained_within(&quad.bounds)
                            })
                            .expect("the mounted focused field outline contains its text mask")
                            .bounds;
                        (covered, field)
                    })
                    .unwrap();
                for blurred in [false, true] {
                    if blurred {
                        app.update_window(handle.into(), |_, window, cx| {
                            assert!(window
                                .dispatch_keystroke(gpui::Keystroke::parse("tab").unwrap(), cx));
                            window.refresh();
                            window.draw(cx).clear(cx);
                        })
                        .unwrap();
                    }
                    handle
                        .update(&mut app, |view, window, cx| {
                            let Some(super::model::Dialog::CopyTo { focus, .. }) =
                                &view.model.dialog
                            else {
                                panic!("the mounted Copy To dialog must remain open");
                            };
                            assert_eq!(
                                *focus,
                                if blurred {
                                    super::model::CopyFocus::Cancel
                                } else {
                                    super::model::CopyFocus::Field
                                },
                                "Tab must leave the destination field before blurred capture"
                            );
                            assert_eq!(view.editing_copy(window, cx), !blurred);
                        })
                        .unwrap();
                    let image = app
                        .capture_screenshot(handle.into())
                        .expect("actual WGPU glyph paint");
                    let foreground = |x, y| {
                        let rgba = image.get_pixel(x, y).0;
                        [198u8, 179, 149]
                            .iter()
                            .zip(&rgba[..3])
                            .all(|(a, b)| a.abs_diff(*b) <= 5)
                    };
                    let (left, right, top, bottom) = (
                        mask.left().0 as u32,
                        mask.right().0 as u32,
                        mask.top().0 as u32,
                        mask.bottom().0 as u32,
                    );
                    let inside = (top..bottom)
                        .flat_map(|y| (left..right).map(move |x| (x, y)))
                        .filter(|(x, y)| foreground(*x, *y))
                        .count();
                    assert!(inside > 8, "real foreground glyph pixels on selection wash: move={moving} base={base} width={width}");
                    // The complete field, outline and two-pixel guard exclude the separate label and buttons.
                    assert!(field.left().0 >= 2. && field.top().0 >= 2.);
                    let guard = (
                        field.left().0.floor() as u32 - 2,
                        field.right().0.ceil() as u32 + 2,
                        field.top().0.floor() as u32 - 2,
                        field.bottom().0.ceil() as u32 + 2,
                    );
                    assert!(guard.1 <= image.width() && guard.3 <= image.height());
                    for y in guard.2..guard.3 {
                        for x in guard.0..guard.1 {
                            if x < left || x >= right || y < top || y >= bottom {
                                assert!(
                                    !foreground(x, y),
                                    "glyph paint escaped the real content mask at ({x}, {y}): move={moving} base={base} width={width} blurred={blurred}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[path = "fix11_vertical_tests.rs"]
mod genuine_vertical_probe;
